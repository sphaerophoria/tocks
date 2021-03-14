mod account;
mod contacts;

use account::Account;
use contacts::{Friend, FriendRequest};

use tocks::{contact::FriendData, AccountData, ChatEvent, ChatView, TocksEvent, TocksUiEvent};

use toxcore::{Message, PublicKey};

use tokio::sync::mpsc::{self, UnboundedReceiver, UnboundedSender};

use std::{
    str::FromStr,
    sync::{Arc, Barrier},
    thread::JoinHandle,
};

use ::log::*;

use qmetaobject::*;

#[derive(QObject, Default)]
#[allow(non_snake_case)]
struct ChatModel {
    base: qt_base_class!(trait QAbstractItemModel),
    friend: qt_property!(QString; NOTIFY friendChanged),
    friendChanged: qt_signal!(),

    chat_view: Option<ChatView>,
}

impl ChatModel {
    fn set_view(&mut self, friend: FriendData, view: ChatView) {
        self.friend = friend.public_key().to_string().into();
        self.friendChanged();

        if let Some(current_view) = &self.chat_view {
            let current_view_guard = current_view.lock();
            if current_view_guard.len() > 0 {
                (self as &dyn QAbstractItemModel).begin_remove_rows(
                    QModelIndex::default(),
                    0,
                    (current_view_guard.len() - 1) as i32,
                );
                (self as &dyn QAbstractItemModel).end_remove_rows();
            }
        }

        self.chat_view = Some(view);

        let view_guard = self.chat_view.as_ref().unwrap().lock();

        if view_guard.len() > 0 {
            (self as &dyn QAbstractItemModel).begin_insert_rows(
                QModelIndex::default(),
                0,
                (view_guard.len() - 1) as i32,
            );
            (self as &dyn QAbstractItemModel).end_insert_rows();
        }
    }

    async fn run(&mut self) {
        if self.chat_view.is_none() {
            return;
        }

        while let Some(event) = self.chat_view.as_mut().unwrap().chat_event().await {
            self.handle_event(event)
        }
    }

    fn handle_event(&self, event: ChatEvent) {
        match event {
            ChatEvent::MessageAdded(idx) => {
                (self as &dyn QAbstractItemModel).begin_insert_rows(
                    QModelIndex::default(),
                    idx as i32,
                    idx as i32,
                );
                (self as &dyn QAbstractItemModel).end_insert_rows();
            }
            ChatEvent::MessageUpdated(idx) => {
                let qmodelidx = (self as &dyn QAbstractItemModel).create_index(idx as i32, 0, 0);
                (self as &dyn QAbstractItemModel).data_changed(qmodelidx, qmodelidx);
            }
        }
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
        if let Some(view) = &self.chat_view {
            view.lock().len() as i32
        } else {
            0
        }
    }

    fn column_count(&self, _parent: QModelIndex) -> i32 {
        1
    }

    fn data(&self, index: QModelIndex, _role: i32) -> QVariant {
        if self.chat_view.is_none() {
            return QVariant::default();
        }

        debug!("Retruning line, {}", index.row());

        let chat_view = self.chat_view.as_ref().unwrap();
        let chat_view_guard = chat_view.lock();

        let message = &chat_view_guard[index.row() as usize];
        if let Message::Normal(message) = &message.message {
            QString::from(message.clone()).to_qvariant()
        } else {
            QVariant::default()
        }
    }
}

#[allow(non_snake_case)]
#[derive(QObject)]
struct QTocks {
    base: qt_base_class!(trait QObject),
    close: qt_method!(fn(&self)),
    addFriendByPublicKey: qt_method!(fn(&self, account: QString, friend: QString)),
    login: qt_method!(fn(&self, account_name: QString, password: QString)),
    newAccount: qt_method!(fn(&self, password: QString)),
    updateChatModel: qt_method!(fn(&self, account: QString, public_key: QString)),
    sendMessage: qt_method!(fn(&self, account: QString, public_key: QString, message: QString)),
    inactiveAccountAdded: qt_signal!(name: QString),
    accountActivated: qt_signal!(account: Account),
    friendAdded: qt_signal!(account: Account, friend: Friend),
    friendRequestReceived: qt_signal!(account: Account, request: FriendRequest),
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
    fn addFriendByPublicKey(&self, account: QString, friend: QString) {
        let self_public_key = PublicKey::from_str(&account.to_string()).unwrap();
        let friend_public_key = PublicKey::from_str(&friend.to_string()).unwrap();
        self.send_ui_request(TocksUiEvent::AddFriendByPublicKey(
            self_public_key,
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
    fn newAccount(&self, password: QString) {
        let password = password.to_string();
        self.send_ui_request(TocksUiEvent::CreateAccount(password));
    }

    #[allow(non_snake_case)]
    fn updateChatModel(&mut self, account: QString, public_key: QString) {
        let account = PublicKey::from_str(&account.to_string()).unwrap();
        let public_key = PublicKey::from_str(&public_key.to_string()).unwrap();

        self.send_ui_request(TocksUiEvent::ChatViewRequested(account, public_key));
    }

    #[allow(non_snake_case)]
    fn sendMessage(&mut self, account: QString, public_key: QString, message: QString) {
        let account = PublicKey::from_str(&account.to_string()).unwrap();
        let public_key = PublicKey::from_str(&public_key.to_string()).unwrap();
        let message = message.to_string();

        self.send_ui_request(TocksUiEvent::MessageSent(account, public_key, message));
    }

    fn set_account_list(&mut self, account_list: Vec<String>) {
        for account in account_list {
            self.inactiveAccountAdded(account.into());
        }
    }

    fn account_login(&mut self, account: AccountData) {
        let qaccount = Account::from(&account);

        self.accountActivated(qaccount);
    }

    fn incoming_friend_request(&mut self, account: AccountData, request: toxcore::FriendRequest) {
        self.friendRequestReceived(
            Account::from(&account),
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
            TocksEvent::AccountLoggedIn(account) => self.account_login(account),
            TocksEvent::FriendAdded(account, friend) => {
                self.friendAdded(Account::from(&account), Friend::from(&friend));
            }
            TocksEvent::ChatView(_account, friend, view) => {
                self.chat_model.pinned().borrow_mut().set_view(friend, view);
            }
        }
    }

    async fn run(&mut self) {
        loop {
            enum Event {
                Tocks(TocksEvent),
                Close,
                None,
            }

            // Some gymastics to help with lifetime management/mutability
            let event = {
                let tocks_event_rx = &mut self.tocks_event_rx;
                let chat_model = &mut self.chat_model;
                let chat_model_pinned = chat_model.pinned();
                let mut chat_model_mut = chat_model_pinned.borrow_mut();

                // chat_model may not be ready yet. In that case it wil
                // return early. We don't want to just rapidly wake up until
                // the model is populated so we drop an infinite future on
                // the end since we know that the sibling branch will wake
                // up when we need it
                let service_chat_model = async {
                    chat_model_mut.run().await;
                    futures::future::pending().await
                };

                tokio::select! {
                    _ = service_chat_model => Event::None,
                    event = tocks_event_rx.recv() => {
                        match event {
                            Some(e) => Event::Tocks(e),
                            None => Event::Close,
                        }
                    },
                }
            };

            match event {
                Event::None => continue,
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
