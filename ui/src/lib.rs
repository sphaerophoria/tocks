mod account;
mod contacts;

use account::Account;

use tocks::{
    audio::{AudioFrame, AudioManager, FormattedAudio, OutputDevice, RepeatingAudioHandle},
    AccountId, CallState, ChatHandle, ChatLogEntry, ChatMessageId, Status, TocksEvent,
    TocksUiEvent, UserHandle,
};

use toxcore::{Message, ToxId};

use anyhow::{Context, Result};

use futures::{
    channel::mpsc::{self, UnboundedSender},
    prelude::*,
};

use std::{
    borrow::BorrowMut,
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    thread::JoinHandle,
};

use ::log::*;

use qmetaobject::*;

fn resource_path<P: AsRef<Path>>(relative_path: P) -> PathBuf {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.join(relative_path.as_ref())
}

fn load_notification_sound() -> FormattedAudio {
    let mut notification_data = Vec::new();
    // FIXME: better error handling
    File::open(resource_path("qml/res/incoming_message.mp3"))
        .unwrap()
        .read_to_end(&mut notification_data)
        .unwrap();

    FormattedAudio::Mp3(notification_data)
}

#[derive(QObject, Default)]
#[allow(non_snake_case)]
struct ChatModel {
    base: qt_base_class!(trait QAbstractItemModel),
    account: qt_property!(i64; NOTIFY accountChanged),
    accountChanged: qt_signal!(),
    chat: qt_property!(i64; NOTIFY chatChanged),
    chatChanged: qt_signal!(),

    chat_log: Vec<ChatLogEntry>,
}

impl ChatModel {
    const MESSAGE_ROLE: i32 = USER_ROLE;
    const SENDER_ID_ROLE: i32 = USER_ROLE + 1;
    const COMPLETE_ROLE: i32 = USER_ROLE + 2;

    fn set_content(&mut self, account_id: AccountId, chat: ChatHandle, content: Vec<ChatLogEntry>) {
        self.account = account_id.id();
        self.accountChanged();

        self.chat = chat.id();
        self.chatChanged();

        (self as &dyn QAbstractItemModel).begin_reset_model();

        self.chat_log = content;

        (self as &dyn QAbstractItemModel).end_reset_model();
    }

    fn push_message(&mut self, entry: ChatLogEntry) {
        (self as &dyn QAbstractItemModel).begin_insert_rows(QModelIndex::default(), 0, 0);

        self.chat_log.push(entry);

        (self as &dyn QAbstractItemModel).end_insert_rows()
    }

    fn resolve_message(&mut self, id: ChatMessageId) {
        let idx = match self.chat_log.binary_search_by(|item| item.id().cmp(&id)) {
            Ok(idx) => idx,
            Err(_) => {
                error!("Chatlog item {} not found", id);
                return;
            }
        };

        self.chat_log[idx].set_complete(true);

        let qidx = (self as &dyn QAbstractItemModel).create_index(
            self.reversed_index(idx as i32) as i32,
            0,
            0,
        );
        (self as &dyn QAbstractItemModel).data_changed(qidx, qidx);
    }

    fn reversed_index(&self, idx: i32) -> usize {
        self.chat_log.len() - idx as usize - 1
    }
}

impl QAbstractItemModel for ChatModel {
    fn index(&self, row: i32, _column: i32, _parent: QModelIndex) -> QModelIndex {
        (self as &dyn QAbstractItemModel).create_index(row, 0, 0)
    }

    fn parent(&self, _index: QModelIndex) -> QModelIndex {
        QModelIndex::default()
    }

    fn row_count(&self, _parent: QModelIndex) -> i32 {
        self.chat_log.len() as i32
    }

    fn column_count(&self, _parent: QModelIndex) -> i32 {
        1
    }

    fn data(&self, index: QModelIndex, role: i32) -> QVariant {
        debug!("Returning line, {}", index.row());

        let entry = self.chat_log.get(self.reversed_index(index.row()));

        if entry.is_none() {
            return QVariant::default();
        }

        let entry = entry.unwrap();

        match role {
            Self::MESSAGE_ROLE => {
                let message = entry.message();

                if let Message::Normal(message) = message {
                    QString::from(message.as_ref()).to_qvariant()
                } else {
                    QVariant::default()
                }
            }
            Self::SENDER_ID_ROLE => entry.sender().id().to_qvariant(),
            Self::COMPLETE_ROLE => entry.complete().to_qvariant(),
            _ => QVariant::default(),
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        let mut ret = HashMap::new();

        ret.insert(Self::MESSAGE_ROLE, "message".into());
        ret.insert(Self::SENDER_ID_ROLE, "senderId".into());
        ret.insert(Self::COMPLETE_ROLE, "complete".into());

        ret
    }
}

// Events to be sent to our internal QTocks loop. We cannot run our QTocks event
// loop from within our class due to qmetaobject mutability issues
enum QTocksEvent {
    SetAudioOutput(OutputDevice),
    PlayNotificationSound,
    StartAudioTest,
    StopAudioTest,
}

#[allow(non_snake_case)]
#[derive(QObject)]
struct QTocks {
    base: qt_base_class!(trait QObject),
    accounts: qt_property!(QVariantList; READ get_accounts NOTIFY accountsChanged),
    accountsChanged: qt_signal!(),
    offlineAccounts: qt_property!(QVariantList; READ get_offline_accounts NOTIFY offlineAccountsChanged),
    offlineAccountsChanged: qt_signal!(),
    newAccount: qt_method!(fn(&mut self, name: QString, password: QString)),
    close: qt_method!(fn(&mut self)),
    addPendingFriend: qt_method!(fn(&mut self, account: i64, user: i64)),
    blockUser: qt_method!(fn(&mut self, account: i64, user: i64)),
    login: qt_method!(fn(&mut self, account_name: QString, password: QString)),
    updateChatModel: qt_method!(fn(&mut self, account: i64, chat: i64)),
    sendMessage: qt_method!(fn(&mut self, account: i64, chat: i64, message: QString)),
    error: qt_signal!(error: QString),
    audioOutputs: qt_property!(QVariantList; READ get_audio_outputs NOTIFY audioOutputsChanged),
    audioOutputsChanged: qt_signal!(),
    startCall: qt_method!(fn(&mut self, account: i64, chat: i64)),
    endCall: qt_method!(fn(&mut self, account: i64, chat: i64)),
    startAudioTest: qt_method!(fn(&mut self)),
    stopAudioTest: qt_method!(fn(&mut self)),
    setAudioOutput: qt_method!(fn(&mut self, output_idx: i64)),
    visible: qt_property!(bool; WRITE set_visible),

    ui_requests_tx: UnboundedSender<TocksUiEvent>,
    qtocks_event_tx: UnboundedSender<QTocksEvent>,
    chat_model: QObjectBox<ChatModel>,
    accounts_storage: HashMap<AccountId, QObjectBox<Account>>,
    offline_accounts: Vec<String>,
    audio_output_storage: Vec<OutputDevice>,
    visible_storage: bool,
}

impl QTocks {
    fn new(
        ui_requests_tx: UnboundedSender<TocksUiEvent>,
        qtocks_event_tx: UnboundedSender<QTocksEvent>,
        audio_devices: Vec<OutputDevice>,
    ) -> QTocks {
        QTocks {
            base: Default::default(),
            accounts: Default::default(),
            accountsChanged: Default::default(),
            offlineAccounts: Default::default(),
            offlineAccountsChanged: Default::default(),
            newAccount: Default::default(),
            close: Default::default(),
            addPendingFriend: Default::default(),
            blockUser: Default::default(),
            login: Default::default(),
            sendMessage: Default::default(),
            updateChatModel: Default::default(),
            error: Default::default(),
            audioOutputs: Default::default(),
            audioOutputsChanged: Default::default(),
            startCall: Default::default(),
            endCall: Default::default(),
            startAudioTest: Default::default(),
            stopAudioTest: Default::default(),
            setAudioOutput: Default::default(),
            visible: Default::default(),
            ui_requests_tx,
            qtocks_event_tx,
            chat_model: QObjectBox::new(Default::default()),
            accounts_storage: Default::default(),
            offline_accounts: Default::default(),
            audio_output_storage: audio_devices,
            visible_storage: false,
        }
    }

    fn close(&mut self) {
        self.send_ui_request(TocksUiEvent::Close);
    }

    #[allow(non_snake_case)]
    fn addPendingFriend(&mut self, account: i64, friend: i64) {
        self.send_ui_request(TocksUiEvent::AcceptPendingFriend(
            AccountId::from(account),
            UserHandle::from(friend),
        ));
    }

    #[allow(non_snake_case)]
    fn blockUser(&mut self, account: i64, user: i64) {
        self.send_ui_request(TocksUiEvent::BlockUser(
            AccountId::from(account),
            UserHandle::from(user),
        ));
    }

    fn login(&mut self, account_name: QString, password: QString) {
        self.send_ui_request(TocksUiEvent::Login(
            account_name.to_string(),
            password.to_string(),
        ));
    }

    #[allow(non_snake_case)]
    fn newAccount(&mut self, name: QString, password: QString) {
        let name = name.to_string();
        let password = password.to_string();
        self.send_ui_request(TocksUiEvent::CreateAccount(name, password));
    }

    #[allow(non_snake_case)]
    fn updateChatModel(&mut self, account: i64, chat_handle: i64) {
        self.send_ui_request(TocksUiEvent::LoadMessages(
            AccountId::from(account),
            ChatHandle::from(chat_handle),
        ));
    }

    #[allow(non_snake_case)]
    fn sendMessage(&mut self, account: i64, chat: i64, message: QString) {
        let message = message.to_string();

        self.send_ui_request(TocksUiEvent::MessageSent(
            AccountId::from(account),
            ChatHandle::from(chat),
            message,
        ));
    }

    fn get_offline_accounts(&mut self) -> QVariantList {
        QPointer::from(&*self).as_pinned().borrow_mut();
        let mut accounts = QVariantList::default();
        accounts.push(QString::from("Create a new account...").to_qvariant());
        for account in &*self.offline_accounts {
            accounts.push(QString::from(account.as_ref()).to_qvariant())
        }

        accounts
    }

    fn set_account_list(&mut self, account_list: Vec<String>) {
        self.offline_accounts = account_list;
        self.offlineAccountsChanged();
    }

    fn account_login(
        &mut self,
        account_id: AccountId,
        user: UserHandle,
        address: ToxId,
        name: String,
    ) {
        let account = QObjectBox::new(Account::new(account_id, user, address, name));
        account.pinned().get_or_create_cpp_object();
        self.accounts_storage.insert(account_id, account);
        self.accountsChanged();
    }

    fn get_accounts(&mut self) -> QVariantList {
        self.accounts_storage
            .values()
            .map(|item| unsafe { (&*item.pinned().borrow() as &dyn QObject).as_qvariant() })
            .collect()
    }

    fn send_ui_request(&mut self, request: TocksUiEvent) {
        if let Err(e) = self.ui_requests_tx.unbounded_send(request) {
            error!("tocks app not responding to UI requests: {}", e);
        }
    }

    fn send_qtocks_request(&mut self, request: QTocksEvent) {
        if let Err(e) = self.qtocks_event_tx.unbounded_send(request) {
            error!("QTocks loop not responding to requests: {}", e);
        }
    }

    fn get_audio_outputs(&mut self) -> QVariantList {
        self.audio_output_storage
            .iter()
            .map(|device| QString::from(device.to_string()).to_qvariant())
            .collect()
    }

    #[allow(non_snake_case)]
    fn setAudioOutput(&mut self, idx: i64) {
        let device = self
            .audio_output_storage
            .get(idx as usize)
            .cloned()
            .expect("Invalid audio device id passed from qml");

        self.send_qtocks_request(QTocksEvent::SetAudioOutput(device));
    }

    #[allow(non_snake_case)]
    fn startCall(&mut self, account: i64, chat: i64) {
        self.send_ui_request(TocksUiEvent::JoinCall(account.into(), chat.into()));
    }

    #[allow(non_snake_case)]
    fn endCall(&mut self, account: i64, chat: i64) {
        self.send_ui_request(TocksUiEvent::LeaveCall(account.into(), chat.into()));
    }

    #[allow(non_snake_case)]
    fn startAudioTest(&mut self) {
        self.send_qtocks_request(QTocksEvent::StartAudioTest);
    }

    #[allow(non_snake_case)]
    fn stopAudioTest(&mut self) {
        self.send_qtocks_request(QTocksEvent::StopAudioTest);
    }

    fn set_visible(&mut self, visible: bool) {
        self.visible_storage = visible
    }

    fn handle_ui_callback(&mut self, event: TocksEvent) {
        match event {
            TocksEvent::AccountListLoaded(list) => self.set_account_list(list),
            TocksEvent::Error(e) => self.error(e.into()),
            TocksEvent::AccountLoggedIn(account_id, user_handle, address, name) => {
                self.account_login(account_id, user_handle, address, name)
            }
            TocksEvent::FriendAdded(account, friend) => {
                self.accounts_storage
                    .get(&account)
                    .unwrap()
                    .pinned()
                    .borrow_mut()
                    .add_friend(&friend);
            }
            TocksEvent::BlockedUserAdded(account, user) => {
                self.accounts_storage
                    .get(&account)
                    .unwrap()
                    .pinned()
                    .borrow_mut()
                    .add_blocked_user(&user);
            }
            TocksEvent::FriendRemoved(account, user_id) => {
                self.accounts_storage
                    .get(&account)
                    .unwrap()
                    .pinned()
                    .borrow_mut()
                    .remove_friend(user_id);
            }
            TocksEvent::MessagesLoaded(account, chat, messages) => {
                self.chat_model
                    .pinned()
                    .borrow_mut()
                    .set_content(account, chat, messages);
            }
            TocksEvent::MessageInserted(account, chat, entry) => {
                let self_id = self
                    .accounts_storage
                    .get(&account)
                    .unwrap()
                    .pinned()
                    .borrow_mut()
                    .self_id();

                if *entry.sender() != self_id && !self.visible_storage {
                    self.send_qtocks_request(QTocksEvent::PlayNotificationSound);
                }

                let chat_model_pinned = self.chat_model.pinned();
                let mut chat_model_ref = chat_model_pinned.borrow_mut();

                if chat_model_ref.account == account.id() && chat_model_ref.chat == chat.id() {
                    chat_model_ref.push_message(entry);
                }
            }
            TocksEvent::MessageCompleted(account, chat, id) => {
                let chat_model_pinned = self.chat_model.pinned();
                let mut chat_model_ref = chat_model_pinned.borrow_mut();
                if chat_model_ref.account == account.id() && chat_model_ref.chat == chat.id() {
                    chat_model_ref.resolve_message(id);
                }
            }
            TocksEvent::FriendStatusChanged(account_id, user_id, status) => {
                self.accounts_storage
                    .get(&account_id)
                    .unwrap()
                    .pinned()
                    .borrow_mut()
                    .set_friend_status(user_id, status);
            }
            TocksEvent::UserNameChanged(account_id, user_id, name) => {
                self.accounts_storage
                    .get(&account_id)
                    .unwrap()
                    .pinned()
                    .borrow_mut()
                    .set_user_name(user_id, &name);
            }
            TocksEvent::ChatCallStateChanged(account_id, chat_handle, state) => {
                // Do nothing
                self.accounts_storage
                    .get(&account_id)
                    .unwrap()
                    .pinned()
                    .borrow_mut()
                    .set_call_state(chat_handle, &state);
            }
            TocksEvent::AudioDataReceived(_, _, _) => {
                // This should be handled by the above layer
                unreachable!();
            }
        }
    }
}

pub struct QmlUi {
    ui_handle: Option<JoinHandle<()>>,
    audio_manager: AudioManager,
    audio_handles: HashMap<(AccountId, ChatHandle), mpsc::UnboundedSender<AudioFrame>>,
    repeating_audio_handle: Option<RepeatingAudioHandle>,
    capture_channel: Option<mpsc::UnboundedReceiver<AudioFrame>>,
    tocks_event_rx: mpsc::UnboundedReceiver<TocksEvent>,
    ui_event_tx: mpsc::UnboundedSender<TocksUiEvent>,
    qtocks_event_rx: mpsc::UnboundedReceiver<QTocksEvent>,
    handle_ui_callback: Box<dyn Fn(TocksEvent) + Send + Sync>,
}

impl QmlUi {
    pub fn new(
        ui_event_tx: mpsc::UnboundedSender<TocksUiEvent>,
        tocks_event_rx: mpsc::UnboundedReceiver<TocksEvent>,
    ) -> Result<QmlUi> {
        let (handle_callback_tx, handle_callback_rx) = std::sync::mpsc::channel();
        let (qtocks_event_tx, qtocks_event_rx) = mpsc::unbounded();

        let mut audio_manager = AudioManager::new().context("Failed to start audio manager")?;
        // Ideally we would trigger something in QTocks when the devices are
        // updated, but at the time of writing we already didn't support it.
        // We'll fix it later.
        let audio_devices = audio_manager
            .output_devices()
            .context("Failed to initialize audio devices")?;

        let ui_event_tx_clone = ui_event_tx.clone();
        // Spawn the QML engine into it's own thread. Our implementation will
        // live on the main thread and be owned directly by the main Tocks
        // instance. Our UI event loop needs to be run independently by Qt so we
        // spawn a new thread and will pass messages back and forth as needed
        let ui_handle = std::thread::spawn(move || {
            let qtocks = QObjectBox::new(QTocks::new(
                ui_event_tx_clone,
                qtocks_event_tx,
                audio_devices,
            ));
            let qtocks_pinned = qtocks.pinned();

            let mut engine = QmlEngine::new();

            engine.set_object_property("tocks".into(), qtocks_pinned);
            engine.set_object_property(
                "chatModel".into(),
                qtocks_pinned.borrow().chat_model.pinned(),
            );

            // FIXME: bundle with qrc on release builds
            engine.load_file(concat!(env!("CARGO_MANIFEST_DIR"), "/qml/Tocks.qml").into());

            let qtocks_clone = QPointer::from(&**qtocks_pinned.borrow_mut());
            let handle_ui_callback = queued_callback(move |event| {
                let pinned = qtocks_clone.as_pinned().unwrap();
                pinned.borrow_mut().handle_ui_callback(event);
            });

            handle_callback_tx
                .send(handle_ui_callback)
                .expect("Failed to hand off ui callback");

            engine.exec();
        });

        let handle_ui_callback = Box::new(handle_callback_rx.recv().unwrap());

        Ok(QmlUi {
            ui_handle: Some(ui_handle),
            audio_manager,
            audio_handles: Default::default(),
            repeating_audio_handle: None,
            capture_channel: None,
            tocks_event_rx,
            ui_event_tx,
            qtocks_event_rx,
            handle_ui_callback,
        })
    }

    pub async fn run(&mut self) {
        loop {
            futures::select! {
                _ = self.audio_manager.run().fuse() => {

                }
                frame = Self::wait_for_capture_frame(&mut self.capture_channel).fuse() => {
                    // Someone else will catch this failure
                    match frame {
                        Some(frame) => {
                            let _ = self.ui_event_tx.unbounded_send(TocksUiEvent::IncomingAudioFrame(frame));
                        },
                        None => {
                            self.capture_channel = None;
                        }
                    }
                }
                event = self.tocks_event_rx.next() => {
                    if event.is_none() {
                        warn!("No more tocks events, stopping event loop");
                        return;
                    }
                    let event = event.unwrap();
                    self.handle_tocks_event(event);
                }
                event = self.qtocks_event_rx.next() => {
                    self.handle_qtocks_event(event)
                }
            }
        }
    }

    async fn wait_for_capture_frame(
        channel: &mut Option<mpsc::UnboundedReceiver<AudioFrame>>,
    ) -> Option<AudioFrame> {
        if let Some(channel) = channel.as_mut() {
            channel.next().await
        } else {
            futures::future::pending().await
        }
    }

    fn handle_qtocks_event(&mut self, event: Option<QTocksEvent>) {
        match event {
            Some(QTocksEvent::SetAudioOutput(device)) => self.set_audio_output(device),
            Some(QTocksEvent::PlayNotificationSound) => self.play_notification_sound(),
            Some(QTocksEvent::StartAudioTest) => self.start_audio_test(),
            Some(QTocksEvent::StopAudioTest) => self.stop_audio_test(),
            None => {
                warn!("No QTocks event received");
            }
        }
    }

    fn handle_tocks_event(&mut self, event: TocksEvent) {
        match event {
            TocksEvent::AudioDataReceived(account, chat, data) => {
                self.handle_audio_data(account, chat, data);
            }
            TocksEvent::ChatCallStateChanged(account, chat, state) => {
                match state {
                    CallState::Active => {
                        // FIXME: error handling
                        if self.audio_handles.get(&(account, chat)).is_none() {
                            let playback_channel =
                                self.audio_manager.create_playback_channel(50).unwrap();
                            self.audio_handles.insert((account, chat), playback_channel);
                        }

                        if self.capture_channel.is_none() {
                            self.capture_channel =
                                Some(self.audio_manager.create_capture_channel().unwrap());
                        }
                    }
                    CallState::Idle | CallState::Incoming | CallState::Outgoing => {
                        self.audio_handles.remove(&(account, chat));
                        if self.audio_handles.is_empty() {
                            self.capture_channel = None;
                        }
                    }
                }
                (*self.handle_ui_callback)(TocksEvent::ChatCallStateChanged(account, chat, state))
            }
            event => (*self.handle_ui_callback)(event),
        };
    }

    fn handle_audio_data(&mut self, account: AccountId, chat: ChatHandle, data: AudioFrame) {
        let handle = self.audio_handles.get(&(account, chat));

        // If handle isn't available we may have left the call
        if let Some(handle) = handle {
            handle.unbounded_send(data).unwrap();
        }
    }

    fn set_audio_output(&mut self, device: OutputDevice) {
        let res = self
            .audio_manager
            .set_output_device(device)
            .context("Failed to set output device");

        if let Err(e) = res {
            (*self.handle_ui_callback)(TocksEvent::Error(e.to_string()));
        }
    }

    fn stop_audio_test(&mut self) {
        self.repeating_audio_handle = None;
    }

    fn start_audio_test(&mut self) {
        self.repeating_audio_handle = Some(
            self.audio_manager
                .play_repeating_formatted_audio(load_notification_sound()),
        );
    }

    fn play_notification_sound(&mut self) {
        self.audio_manager
            .play_formatted_audio(load_notification_sound());
    }
}

impl Drop for QmlUi {
    fn drop(&mut self) {
        let mut handle = None;
        std::mem::swap(&mut handle, &mut self.ui_handle);
        if let Some(handle) = handle {
            handle.join().unwrap();
        }
    }
}

pub(crate) fn status_to_qstring(status: &Status) -> QString {
    match status {
        Status::Online => "online".into(),
        Status::Away => "away".into(),
        Status::Busy => "busy".into(),
        Status::Offline => "offline".into(),
        Status::Pending => "pending".into(),
    }
}

pub(crate) fn call_state_to_qtring(state: &CallState) -> QString {
    match state {
        CallState::Active => "active".into(),
        CallState::Incoming => "incoming".into(),
        CallState::Idle => "idle".into(),
        CallState::Outgoing => "outgoing".into(),
    }
}
