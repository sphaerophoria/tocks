#![recursion_limit="256"]

mod account;
mod chat_model;
mod contacts;

use account::Account;

use chat_model::ChatModel;
use chrono::{DateTime, Local, Utc, NaiveDateTime, TimeZone};
use tocks::{
    audio::{AudioFrame, AudioManager, FormattedAudio, OutputDevice, RepeatingAudioHandle},
    AccountId, CallState, ChatHandle, Status, TocksEvent,
    TocksUiEvent, UserHandle,
};

use toxcore::{ToxId};

use anyhow::{Context, Result};

use futures::{
    channel::mpsc::{self, UnboundedSender},
    prelude::*,
};

use notify_rust::{Notification, NotificationHandle};

use std::{
    cell::RefCell,
    collections::HashMap,
    fs::File,
    io::Read,
    path::{Path, PathBuf},
    pin::Pin,
    thread::JoinHandle,
};

use ::log::*;

use qmetaobject::*;

cpp::cpp!{{
    #include <memory>
    #include <iostream>
    #include <QtWidgets/QApplication>
    #include <QtGui/QIcon>
}}

// FIXME: Add these as qrcs
const ATTRIBUTION: &'static str = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/qml/res/attribution.txt"));
const ICON_PATH: &'static str = concat!(env!("CARGO_MANIFEST_DIR"), "/qml/res/tox-logo.svg");

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

// Events to be sent to our internal QTocks loop. We cannot run our QTocks event
// loop from within our class due to qmetaobject mutability issues
enum QTocksEvent {
    SetAudioOutput(OutputDevice),
    SendNotification(String, String),
    StartAudioTest,
    StopAudioTest,
}

#[allow(non_snake_case)]
#[derive(QObject)]
struct QTocks {
    base: qt_base_class!(trait QObject),
    attribution: qt_property!(QString; CONST READ get_attribution),
    accounts: qt_property!(QVariantList; READ get_accounts NOTIFY accountsChanged),
    accountsChanged: qt_signal!(),
    offlineAccounts: qt_property!(QVariantList; READ get_offline_accounts NOTIFY offlineAccountsChanged),
    offlineAccountsChanged: qt_signal!(),
    newAccount: qt_method!(fn(&mut self, name: QString, password: QString)),
    close: qt_method!(fn(&mut self)),
    addPendingFriend: qt_method!(fn(&mut self, account: i64, user: i64)),
    blockUser: qt_method!(fn(&mut self, account: i64, user: i64)),
    login: qt_method!(fn(&mut self, account_name: QString, password: QString)),
    sendMessage: qt_method!(fn(&mut self, account: i64, chat: i64, message: QString)),
    error: qt_signal!(error: QString),
    audioOutputs: qt_property!(QVariantList; READ get_audio_outputs NOTIFY audioOutputsChanged),
    audioOutputsChanged: qt_signal!(),
    startCall: qt_method!(fn(&mut self, account: i64, chat: i64)),
    endCall: qt_method!(fn(&mut self, account: i64, chat: i64)),
    startAudioTest: qt_method!(fn(&mut self)),
    stopAudioTest: qt_method!(fn(&mut self)),
    setAudioOutput: qt_method!(fn(&mut self, output_idx: i64)),
    markChatRead: qt_method!(fn(&mut self, account: i64, chat: i64, timestamp: QDateTime)),
    sendNotification: qt_method!(fn(&mut self, title: QString, message: QString)),

    ui_requests_tx: UnboundedSender<TocksUiEvent>,
    qtocks_event_tx: UnboundedSender<QTocksEvent>,
    accounts_storage: HashMap<AccountId, Pin<Box<RefCell<Account>>>>,
    offline_accounts: Vec<String>,
    audio_output_storage: Vec<OutputDevice>,
}

impl QTocks {
    fn new(
        ui_requests_tx: UnboundedSender<TocksUiEvent>,
        qtocks_event_tx: UnboundedSender<QTocksEvent>,
        audio_devices: Vec<OutputDevice>,
    ) -> QTocks {
        QTocks {
            base: Default::default(),
            attribution: Default::default(),
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
            error: Default::default(),
            audioOutputs: Default::default(),
            audioOutputsChanged: Default::default(),
            startCall: Default::default(),
            endCall: Default::default(),
            startAudioTest: Default::default(),
            stopAudioTest: Default::default(),
            setAudioOutput: Default::default(),
            markChatRead: Default::default(),
            sendNotification: Default::default(),
            ui_requests_tx,
            qtocks_event_tx,
            accounts_storage: Default::default(),
            offline_accounts: Default::default(),
            audio_output_storage: audio_devices,
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
    fn sendMessage(&mut self, account: i64, chat: i64, message: QString) {
        let message = message.to_string();

        self.send_ui_request(TocksUiEvent::MessageSent(
            AccountId::from(account),
            ChatHandle::from(chat),
            message,
        ));
    }

    fn get_offline_accounts(&mut self) -> QVariantList {
        let mut accounts = QVariantList::default();
        accounts.push(QString::from("Create a new account...").to_qvariant());
        for account in &*self.offline_accounts {
            accounts.push(QString::from(account.as_ref()).to_qvariant())
        }

        accounts
    }

    fn get_attribution(&mut self) -> QString {
        ATTRIBUTION.into()
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
        let account = Box::pin(RefCell::new(Account::new(account_id, user, address, name)));
        unsafe { QObject::cpp_construct(&account) };
        self.accounts_storage.insert(account_id, account);
        self.accountsChanged();
    }

    fn get_accounts(&mut self) -> QVariantList {
        self.accounts_storage
            .values_mut()
            .map(|item| unsafe { (&*item.get_mut() as &dyn QObject).as_qvariant() })
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

    #[allow(non_snake_case)]
    fn markChatRead(&mut self, account: i64, chat: i64, timestamp: QDateTime) {
        let (date, time) = timestamp.get_date_time();
        let date = date.into();
        let time = time.into();
        let datetime = NaiveDateTime::new(date, time);
        let localtime = Local.from_local_datetime(&datetime).unwrap();
        let utctime = localtime.with_timezone(&Utc);
        self.send_ui_request(TocksUiEvent::MarkChatRead(account.into(), chat.into(), utctime));
    }

    #[allow(non_snake_case)]
    fn sendNotification(&mut self, title: QString, message: QString) {
        self.send_qtocks_request(QTocksEvent::SendNotification(title.into(), message.into()));
    }

    fn handle_ui_callback(&mut self, event: TocksEvent) {
        match event {
            TocksEvent::AccountListLoaded(list) => self.set_account_list(list),
            TocksEvent::Error(e) => self.error(e.into()),
            TocksEvent::AccountLoggedIn(account_id, user_handle, address, name) => {
                self.account_login(account_id, user_handle, address, name)
            }
            TocksEvent::FriendAdded(account, friend) => {
                let tx_clone = self.ui_requests_tx.clone();
                let chat_id = *friend.chat_handle();
                let chat_model = ChatModel::new(move |id| {
                    let res = tx_clone.unbounded_send(TocksUiEvent::LoadMessages{
                        account: account,
                        chat: chat_id,
                        num_messages: 50usize,
                        start_id: id
                    });

                    if let Err(e) = res {
                        error!("Failed to request more messages from tocks");
                    }
                });

                self.accounts_storage
                    .get(&account)
                    .unwrap()
                    .borrow_mut()
                    .add_friend(&friend, chat_model);
            }
            TocksEvent::BlockedUserAdded(account, user) => {
                self.accounts_storage
                    .get(&account)
                    .unwrap()
                    .borrow_mut()
                    .add_blocked_user(&user);
            }
            TocksEvent::FriendRemoved(account, user_id) => {
                self.accounts_storage
                    .get(&account)
                    .unwrap()
                    .borrow_mut()
                    .remove_friend(user_id);
            }
            TocksEvent::MessagesLoaded(account, chat, messages) => {
                self.accounts_storage
                    .get(&account)
                    .unwrap()
                    .borrow_mut()
                    .chat_from_chat_handle(&chat)
                    .unwrap()
                    .borrow_mut()
                    .push_messages(messages);
            }
            TocksEvent::MessageInserted(account, chat, entry) => {
                self.accounts_storage
                    .get(&account)
                    .unwrap()
                    .borrow_mut()
                    .chat_from_chat_handle(&chat)
                    .unwrap()
                    .borrow_mut()
                    .push_message(entry);

            }
            TocksEvent::MessageCompleted(account, chat, id) => {
                self.accounts_storage
                    .get(&account)
                    .unwrap()
                    .borrow_mut()
                    .chat_from_chat_handle(&chat)
                    .unwrap()
                    .borrow_mut()
                    .resolve_message(id);
            }
            TocksEvent::FriendStatusChanged(account_id, user_id, status) => {
                self.accounts_storage
                    .get(&account_id)
                    .unwrap()
                    .borrow_mut()
                    .friend_from_user_handle(&user_id)
                    .unwrap()
                    .borrow_mut()
                    .set_status(status);
            }
            TocksEvent::UserNameChanged(account_id, user_id, name) => {
                self.accounts_storage
                    .get(&account_id)
                    .unwrap()
                    .borrow_mut()
                    .friend_from_user_handle(&user_id)
                    .unwrap()
                    .borrow_mut()
                    .set_name(&name);
            }
            TocksEvent::ChatCallStateChanged(account_id, chat_handle, state) => {
                self.accounts_storage
                    .get(&account_id)
                    .unwrap()
                    .borrow_mut()
                    .chat_from_chat_handle(&chat_handle)
                    .unwrap()
                    .borrow_mut()
                    .set_call_state(&state);
            }
            TocksEvent::AudioDataReceived(_, _, _) => {
                // This should be handled by the above layer
            }
            TocksEvent::ChatReadTimeUpdated(account, chat_handle, timestamp) => {
                self.accounts_storage
                    .get(&account)
                    .unwrap()
                    .borrow_mut()
                    .chat_from_chat_handle(&chat_handle)
                    .unwrap()
                    .borrow_mut()
                    .set_last_read_time(timestamp);
            }
        }
    }
}

pub struct QmlUi {
    ui_handle: Option<JoinHandle<()>>,
    audio_manager: AudioManager,
    audio_handles: HashMap<(AccountId, ChatHandle), mpsc::UnboundedSender<AudioFrame>>,
    notification: Option<NotificationHandle>,
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

            let icon_path = std::ffi::CString::new(ICON_PATH).unwrap();
            let icon_path = icon_path.as_ptr();

            unsafe {
                cpp::cpp! ( [icon_path as "const char *"] {
                    QApplication* app = qobject_cast<QApplication*>(QCoreApplication::instance());
                    app->setWindowIcon(QIcon(icon_path));
                })
            }

            engine.set_object_property("tocks".into(), qtocks_pinned);

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
            notification: None,
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
                    if let Err(e) = self.handle_qtocks_event(event) {
                        error!("Failed to handle qtocks event: {:?}", e);
                    }
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

    fn handle_qtocks_event(&mut self, event: Option<QTocksEvent>) -> Result<()> {
        match event {
            Some(QTocksEvent::SetAudioOutput(device)) => self.set_audio_output(device),
            Some(QTocksEvent::SendNotification(title, message)) => {
                self.notification = match self.notification.take() {
                    Some(mut handle) => {
                        handle
                            .summary(&title)
                            .icon(ICON_PATH)
                            .body(&message);
                        handle.update();
                        Some(handle)

                    }
                    None => {
                        Some(Notification::new()
                            .appname("Tocks")
                            .summary(&title)
                            .icon(ICON_PATH)
                            .body(&message)
                            .show()?)
                    }
                };
                self.play_notification_sound()
            }
            Some(QTocksEvent::StartAudioTest) => self.start_audio_test(),
            Some(QTocksEvent::StopAudioTest) => self.stop_audio_test(),
            None => {
                warn!("No QTocks event received");
            }
        }

        Ok(())
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
