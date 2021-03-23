mod account;
mod contacts;

use account::Account;
use contacts::{Friend, FriendRequest};

use tocks::{
    AccountId, ChatHandle, ChatLogEntry, ChatMessageId, TocksEvent, TocksUiEvent, UserHandle,
};

use toxcore::{Message, PublicKey, ToxId, Status};

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use std::{
    str::FromStr,
    collections::HashMap,
    sync::{Arc, Barrier},
    thread::JoinHandle,
};

use ::log::*;

use qmetaobject::*;

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

    fn set_content(&mut self, account_id: AccountId, chat: ChatHandle, content: Vec<ChatLogEntry>) {
        self.account = account_id.id();
        self.accountChanged();

        self.chat = chat.id();
        self.chatChanged();

        if self.chat_log.len() > 0 {
            (self as &dyn QAbstractItemModel).begin_remove_rows(
                QModelIndex::default(),
                0,
                (self.chat_log.len() - 1) as i32,
            );
            (self as &dyn QAbstractItemModel).end_remove_rows();
        }

        self.chat_log = content;

        if self.chat_log.len() > 0 {
            (self as &dyn QAbstractItemModel).begin_insert_rows(
                QModelIndex::default(),
                0,
                (self.chat_log.len() - 1) as i32,
            );
            (self as &dyn QAbstractItemModel).end_insert_rows();
        }
    }

    fn push_message(&mut self, entry: ChatLogEntry) {
        (self as &dyn QAbstractItemModel).begin_insert_rows(
            QModelIndex::default(),
            0,
            0,
        );

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

        let qidx = (self as &dyn QAbstractItemModel).create_index(idx as i32, 0, 0);
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
            },
            Self::SENDER_ID_ROLE => {
                entry.sender().id().to_qvariant()
            }
            _ => QVariant::default()
        }
    }

    fn role_names(&self) -> HashMap<i32, QByteArray> {
        let mut ret = HashMap::new();

        ret.insert(Self::MESSAGE_ROLE, "message".into());
        ret.insert(Self::SENDER_ID_ROLE, "senderId".into());

        ret
    }
}

#[allow(non_snake_case)]
#[derive(QObject)]
struct QTocks {
    base: qt_base_class!(trait QObject),
    close: qt_method!(fn(&self)),
    addFriendByPublicKey: qt_method!(fn(&self, account: i64, friend: QString)),
    login: qt_method!(fn(&self, account_name: QString, password: QString)),
    newAccount: qt_method!(fn(&self, name: QString, password: QString)),
    updateChatModel: qt_method!(fn(&self, account: i64, chat: i64)),
    sendMessage: qt_method!(fn(&self, account: i64, chat: i64, message: QString)),
    inactiveAccountAdded: qt_signal!(name: QString),
    accountActivated: qt_signal!(account: Account),
    friendAdded: qt_signal!(account: i64, friend: Friend),
    friendRequestReceived: qt_signal!(account: i64, request: FriendRequest),
    friendStatusChanged: qt_signal!(accountId: i64, friendId: i64, status: QString),
    error: qt_signal!(error: QString),

    ui_requests_tx: UnboundedSender<TocksUiEvent>,
    tocks_event_rx: UnboundedReceiver<TocksEvent>,
    chat_model: QObjectBox<ChatModel>,
}

impl QTocks {
    fn new(
        ui_requests_tx: UnboundedSender<TocksUiEvent>,
        tocks_event_rx: UnboundedReceiver<TocksEvent>,
    ) -> QTocks {
        QTocks {
            base: Default::default(),
            close: Default::default(),
            addFriendByPublicKey: Default::default(),
            login: Default::default(),
            newAccount: Default::default(),
            sendMessage: Default::default(),
            inactiveAccountAdded: Default::default(),
            updateChatModel: Default::default(),
            accountActivated: Default::default(),
            friendAdded: Default::default(),
            friendRequestReceived: Default::default(),
            friendStatusChanged: Default::default(),
            error: Default::default(),
            ui_requests_tx,
            tocks_event_rx,
            chat_model: QObjectBox::new(Default::default()),
        }
    }

    fn close(&self) {
        self.send_ui_request(TocksUiEvent::Close);
    }

    #[allow(non_snake_case)]
    fn addFriendByPublicKey(&self, account: i64, friend: QString) {
        let self_public_key = PublicKey::from_str(&account.to_string()).unwrap();
        let friend_public_key = PublicKey::from_str(&friend.to_string()).unwrap();
        self.send_ui_request(TocksUiEvent::AddFriendByPublicKey(
            AccountId::from(account),
            friend_public_key,
        ));
    }

    fn login(&self, account_name: QString, password: QString) {
        self.send_ui_request(TocksUiEvent::Login(
            account_name.to_string(),
            password.to_string(),
        ));
    }

    #[allow(non_snake_case)]
    fn newAccount(&self, name: QString, password: QString) {
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

    fn set_account_list(&mut self, account_list: Vec<String>) {
        for account in account_list {
            self.inactiveAccountAdded(account.into());
        }
    }

    fn account_login(
        &mut self,
        account_id: AccountId,
        user: UserHandle,
        address: ToxId,
        name: String,
    ) {
        let qaccount = Account {
            id: account_id.id(),
            userId: user.id(),
            toxId: address.to_string().into(),
            name: name.into(),
        };

        self.accountActivated(qaccount);
    }

    fn incoming_friend_request(&mut self, account: AccountId, request: toxcore::FriendRequest) {
        self.friendRequestReceived(
            account.id(),
            FriendRequest {
                sender: request.public_key.to_string().into(),
                message: request.message.into(),
            },
        );
    }

    fn send_ui_request(&self, request: TocksUiEvent) {
        if let Err(e) = self.ui_requests_tx.send(request) {
            error!("tocks app not responding to UI requests: {}", e);
        }
    }

    fn handle_ui_callback(&mut self, event: TocksEvent) {
        match event {
            TocksEvent::AccountListLoaded(list) => self.set_account_list(list),
            TocksEvent::Error(e) => self.error(e.into()),
            TocksEvent::FriendRequestReceived(account, request) => {
                self.incoming_friend_request(account, request)
            }
            TocksEvent::AccountLoggedIn(account_id, user_handle, address, name) => {
                self.account_login(account_id, user_handle, address, name)
            }
            TocksEvent::FriendAdded(account, friend) => {
                self.friendAdded(account.id(), Friend::from(&friend));
            }
            TocksEvent::MessagesLoaded(account, chat, messages) => {
                self.chat_model
                    .pinned()
                    .borrow_mut()
                    .set_content(account, chat, messages);
            }
            TocksEvent::MessageInserted(account, chat, entry) => {
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
                self.friendStatusChanged(account_id.id(), user_id.id(), status_to_qstring(&status));
            }
        }
    }

    async fn run(&mut self) {
        loop {
            enum Event {
                Tocks(TocksEvent),
                Close,
            }

            // Some gymastics to help with lifetime management/mutability
            let event = {
                let tocks_event_rx = &mut self.tocks_event_rx;

                tokio::select! {
                    event = tocks_event_rx.recv() => {
                        match event {
                            Some(e) => Event::Tocks(e),
                            None => Event::Close,
                        }
                    },
                }
            };

            match event {
                Event::Close => break,
                Event::Tocks(event) => self.handle_ui_callback(event),
            }
        }
    }
}

pub struct QmlUi {
    ui_handle: Option<JoinHandle<()>>,
}

impl QmlUi {
    pub fn new(
        ui_event_tx: mpsc::UnboundedSender<TocksUiEvent>,
        tocks_event_rx: mpsc::UnboundedReceiver<TocksEvent>,
    ) -> QmlUi {
        // barrier used to ensure we do not claim to be complete until the qml thread is servicing the tocks events
        let barrier = Arc::new(Barrier::new(2));
        let qt_barrier = Arc::clone(&barrier);

        // Spawn the QML engine into it's own thread. Our implementation will
        // live on the main thread and be owned directly by the main Tocks
        // instance. Our UI event loop needs to be run independently by Qt so we
        // spawn a new thread and will pass messages back and forth as needed
        let ui_handle = std::thread::spawn(move || {
            let qtocks = QObjectBox::new(QTocks::new(ui_event_tx, tocks_event_rx));
            let qtocks_pinned = qtocks.pinned();

            contacts::FriendRequest::register(None);

            let mut engine = QmlEngine::new();

            engine.set_object_property("tocks".into(), qtocks_pinned);
            engine.set_object_property(
                "chatModel".into(),
                qtocks_pinned.borrow().chat_model.pinned(),
            );

            execute_async(async move {
                let qtocks_pinned = qtocks.pinned();
                qtocks_pinned.borrow_mut().run().await;
            });

            // FIXME: bundle with qrc on release builds
            engine.load_file(concat!(env!("CARGO_MANIFEST_DIR"), "/qml/Tocks.qml").into());

            qt_barrier.wait();

            engine.exec();
        });

        barrier.wait();

        QmlUi {
            ui_handle: Some(ui_handle),
        }
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
    }
}

