// Seems to trigger incorrectly frequently
#![allow(clippy::needless_lifetimes)]

pub mod contact;
pub mod error;

mod chatroom;

pub use crate::{
    chatroom::{ChatEvent, ChatView},
    error::{Error, Result},
};

use crate::{chatroom::ChatRoom, contact::FriendData};

use toxcore::{Friend, FriendRequest, Message, PublicKey, Tox, ToxId};

use futures::FutureExt;
use log::*;
use platform_dirs::AppDirs;
use tokio::sync::{broadcast, mpsc};

use std::{
    collections::HashMap,
    fs::{self, File},
    io::Read,
    path::PathBuf,
};

// UI things that tocks will need to react to
pub enum TocksUiEvent {
    Close,
    CreateAccount(String /*password*/),
    AddFriendByPublicKey(
        PublicKey, /*self address*/
        PublicKey, /*friend address*/
    ),
    Login(String /* Tox account name */, String /*password*/),
    ChatViewRequested(PublicKey /*self*/, PublicKey /* friend */),
    MessageSent(
        PublicKey, /*self*/
        PublicKey, /* friend */
        String,    /* message */
    ),
}

// Things external observers (like the UI) may want to observe
pub enum TocksEvent {
    Error(String),
    AccountListLoaded(Vec<String>),
    AccountLoggedIn(AccountData),
    FriendRequestReceived(AccountData, FriendRequest),
    FriendAdded(AccountData, FriendData),
    ChatView(AccountData, FriendData, ChatView),
}

// Things that Tocks can handle in it's core iteration loop
enum Event<'a> {
    Ui(TocksUiEvent),
    FriendRequest(&'a Account, toxcore::FriendRequest),
    None,
}

pub struct Tocks {
    _appdirs: AppDirs,
    tox_save_dir: PathBuf,
    accounts: HashMap<PublicKey, Account>,
    ui_event_rx: mpsc::UnboundedReceiver<TocksUiEvent>,
    tocks_event_tx: mpsc::UnboundedSender<TocksEvent>,
}

impl Tocks {
    pub fn new(
        ui_event_rx: mpsc::UnboundedReceiver<TocksUiEvent>,
        tocks_event_tx: mpsc::UnboundedSender<TocksEvent>,
    ) -> Tocks {
        let tox_save_dir = AppDirs::new(Some("tox"), false).unwrap().config_dir;

        let tocks = Tocks {
            _appdirs: AppDirs::new(Some("tocks"), false).unwrap(),
            tox_save_dir,
            accounts: HashMap::new(),
            ui_event_rx,
            tocks_event_tx,
        };

        let account_list = tocks.retrieve_account_list().unwrap_or_default();
        Self::send_tocks_event(
            &tocks.tocks_event_tx,
            TocksEvent::AccountListLoaded(account_list),
        );

        tocks
    }

    pub async fn run(&mut self) {
        loop {
            // Split struct for easier reference management
            let ui_event_rx = &mut self.ui_event_rx;
            let account = &mut self.accounts;

            match Self::next_event(account, ui_event_rx).await {
                Event::Ui(request) => match self.handle_ui_request(request) {
                    Ok(true) => return,
                    Ok(false) => (),
                    Err(e) => {
                        error!("Failed to handle UI request: {}", e);
                        Self::send_tocks_event(
                            &self.tocks_event_tx,
                            TocksEvent::Error(e.to_string()),
                        );
                    }
                },
                Event::FriendRequest(account, friend_request) => {
                    info!("Received friend request from {}", friend_request.public_key);

                    Self::send_tocks_event(
                        &self.tocks_event_tx,
                        TocksEvent::FriendRequestReceived(account.data().clone(), friend_request),
                    );
                }
                Event::None => (),
            }
        }
    }

    /// Returns `true` if app should be closed
    pub fn handle_ui_request(&mut self, event: TocksUiEvent) -> Result<bool> {
        match event {
            TocksUiEvent::Close => {
                return Ok(true);
            }
            TocksUiEvent::CreateAccount(password) => {
                if !password.is_empty() {
                    warn!("Account passwords are not yet supported");
                }
                let account = self.create_account()?;
                Self::send_tocks_event(
                    &self.tocks_event_tx,
                    TocksEvent::AccountLoggedIn(self.accounts[&account].data().clone()),
                );
            }
            TocksUiEvent::AddFriendByPublicKey(self_address, friend_address) => {
                let account = self.accounts.get_mut(&self_address);

                match account {
                    Some(account) => {
                        account.add_friend_publickey(&friend_address)?;

                        Self::send_tocks_event(
                            &self.tocks_event_tx,
                            TocksEvent::FriendAdded(
                                account.data().clone(),
                                FriendData::from(&account.friends()[&friend_address]),
                            ),
                        );
                    }
                    None => {
                        error!("Account {} not present", self_address);
                    }
                }
            }
            TocksUiEvent::Login(account_name, password) => {
                // FIXME: implement login
                let account = self.open_account(account_name, password)?;
                let account = &self.accounts[&account];
                Self::send_tocks_event(
                    &self.tocks_event_tx,
                    TocksEvent::AccountLoggedIn(account.data().clone()),
                );

                for friend in account.friends().values() {
                    Self::send_tocks_event(
                        &self.tocks_event_tx,
                        TocksEvent::FriendAdded(account.data().clone(), FriendData::from(friend)),
                    );
                }
            }
            TocksUiEvent::ChatViewRequested(account, public_key) => {
                let account = self.accounts.get(&account);

                let friend_data = account
                    .and_then(|account| account.friends().get(&public_key))
                    .map(FriendData::from);

                let view = account.and_then(|account| account.chatview(&public_key));

                if let (Some(account), Some(friend_data), Some(view)) = (account, friend_data, view)
                {
                    Self::send_tocks_event(
                        &self.tocks_event_tx,
                        TocksEvent::ChatView(account.data().clone(), friend_data, view),
                    );
                } else {
                    error!("Requested chat view does not exist")
                }
            }
            TocksUiEvent::MessageSent(account_key, friend_key, message) => {
                let account = self.accounts.get_mut(&account_key);

                if let Some(account) = account {
                    account.send_message(&friend_key, message)?;
                } else {
                    error!("Could not send to {} from {}", friend_key, account_key);
                }
            }
        };

        Ok(false)
    }

    /// Return a usize instead of the account ref directly so that the outer
    /// functions can split the Tocks mutable reference
    fn create_account(&mut self) -> Result<PublicKey> {
        let account = Account::create()?;

        info!("Created account: {}", account.tox.self_address());
        let public_key = account.tox.self_public_key();
        self.accounts.insert(public_key.clone(), account);

        Ok(public_key)
    }

    fn open_account(&mut self, mut account_name: String, password: String) -> Result<PublicKey> {
        account_name.push_str(".tox");
        let account_file_path = self.tox_save_dir.join(account_name);
        let mut account_file = File::open(account_file_path)?;
        let account = Account::login(&mut account_file, password)?;

        info!("Logged into account: {}", account.data().tox_id);

        let public_key = account.tox.self_public_key();
        self.accounts.insert(public_key.clone(), account);

        Ok(public_key)
    }

    fn retrieve_account_list(&self) -> Result<Vec<String>> {
        let mut accounts: Vec<String> = fs::read_dir(&self.tox_save_dir)?
            .filter(|entry| entry.is_ok())
            .filter_map(|entry| entry.unwrap().file_name().into_string().ok())
            .filter(|item| item.ends_with(".tox"))
            .map(|item| item[..item.len() - 4].to_string())
            .collect();

        accounts.sort();

        Ok(accounts)
    }

    fn send_tocks_event(tocks_event_tx: &mpsc::UnboundedSender<TocksEvent>, event: TocksEvent) {
        // We don't really care if this fails, who am I to say whether or not an
        // external library wants to service my events
        let _ = tocks_event_tx.send(event);
    }

    async fn next_event<'a>(
        accounts: &'a mut HashMap<PublicKey, Account>,
        ui_events: &mut mpsc::UnboundedReceiver<TocksUiEvent>,
    ) -> Event<'a> {
        let account_events = if accounts.is_empty() {
            // futures::future::select_all is not happy with 0 elements
            futures::future::pending().boxed()
        } else {
            futures::future::select_all(accounts.values_mut().map(|ac| ac.run().boxed())).boxed()
        };

        let event = tokio::select! {
            request = ui_events.recv() => {
                match request {
                    Some(request) => Event::Ui(request),
                    None => {
                        error!("Unexpected dropped UI requester, closing app");
                        Event::Ui(TocksUiEvent::Close)
                    },
                }
            },
            event = account_events => {
                // Select all will return a tuple of the result + the
                // remaining. We'll retrieve the remaining ourselves
                // next iteration so we only need to extract the value
                // that actually resolved
                event.0
            }
        };

        event
    }
}

pub type Friends = HashMap<PublicKey, Friend>;

#[derive(Clone, Debug)]
pub struct AccountData {
    public_key: PublicKey,
    tox_id: ToxId,
    name: String,
}

impl AccountData {
    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn tox_id(&self) -> &ToxId {
        &self.tox_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }
}

pub struct Account {
    tox: Tox,
    friends: Friends,
    chatrooms: HashMap<PublicKey, ChatRoom>,
    friend_requests: broadcast::Receiver<FriendRequest>,
    incoming_messages: HashMap<PublicKey, broadcast::Receiver<Message>>,
    data: AccountData,
}

impl Account {
    fn create() -> Result<Account> {
        let tox = Tox::builder()?.build()?;
        Self::from_tox(tox)
    }

    fn login<T: Read>(account_buf: &mut T, password: String) -> Result<Account> {
        let mut account_vec = Vec::new();
        account_buf.read_to_end(&mut account_vec)?;

        if !password.is_empty() {
            return Err(Error::Unimplemented("Login with password".into()));
        }

        let tox = Tox::builder()?
            .savedata(toxcore::SaveData::ToxSave(&account_vec))
            .build()?;

        Self::from_tox(tox)
    }

    fn from_tox(mut tox: Tox) -> Result<Account> {
        let friend_requests = tox.friend_requests();
        let self_public_key = tox.self_public_key();
        let tox_id = tox.self_address();
        let mut name = tox.self_name();

        if name.is_empty() {
            name = self_public_key.to_string();
        }

        let friends = tox
            .friends()?
            .into_iter()
            .map(|friend| (friend.public_key(), friend))
            .collect::<HashMap<_, _>>();

        let chatrooms = friends
            .iter()
            .map(|(key, _friend)| (key.clone(), ChatRoom::new()))
            .collect();

        let incoming_messages = friends
            .values()
            .map(|friend| {
                (
                    friend.public_key().clone(),
                    tox.incoming_friend_messages(friend),
                )
            })
            .collect();

        Ok(Account {
            tox,
            friends,
            chatrooms,
            friend_requests,
            incoming_messages,
            data: AccountData {
                public_key: self_public_key,
                tox_id,
                name,
            },
        })
    }

    fn data(&self) -> &AccountData {
        &self.data
    }

    fn friends(&self) -> &Friends {
        &self.friends
    }

    fn chatview(&self, key: &PublicKey) -> Option<ChatView> {
        self.chatrooms.get(key).map(|item| item.view())
    }

    fn add_friend_publickey(&mut self, friend_key: &PublicKey) -> Result<()> {
        // FIXME: eventually need to sync state with toxcore, re-add from DB etc.
        let f = self.tox.add_friend_norequest(&friend_key)?;
        let public_key = f.public_key();

        self.friends.insert(public_key.clone(), f);
        let f = &self.friends[&public_key];

        self.chatrooms.insert(public_key.clone(), ChatRoom::new());
        self.incoming_messages
            .insert(public_key, self.tox.incoming_friend_messages(&f));

        Ok(())
    }

    fn send_message(&mut self, friend: &PublicKey, message: String) -> Result<()> {
        let friend = self.friends.get(friend);
        if friend.is_none() {
            return Err(Error::InvalidArgument);
        }
        let friend = friend.unwrap();

        let message = Message::Normal(message);
        let receipt = self.tox.send_message(&friend, &message)?;

        let chatroom = self
            .chatrooms
            .entry(friend.public_key())
            .or_insert_with(|| ChatRoom::new());

        chatroom.push_sent_message(message, receipt);

        Ok(())
    }

    async fn handle_incoming_messages(
        incoming_messages: &mut HashMap<PublicKey, broadcast::Receiver<Message>>,
        chatrooms: &mut HashMap<PublicKey, ChatRoom>,
    ) {
        if incoming_messages.is_empty() {
            futures::future::pending::<()>().await;
        }

        let next_message = incoming_messages.iter_mut().map(|(key, receiver)| {
            async move {
                let result = receiver.recv().await;
                result.map(|message| (key, message))
            }
            .boxed()
        });

        let result = futures::future::select_all(next_message).await.0;
        if let Ok((public_key, message)) = result {
            let chatroom = chatrooms
                .entry(public_key.clone())
                .or_insert_with(|| ChatRoom::new());
            chatroom.push_received_message(message)
        }
    }

    async fn run<'a>(&'a mut self) -> Event<'a> {
        tokio::select! {
            _ = self.tox.run() => { Event::None }
            friend_request = self.friend_requests.recv() => {
                match friend_request {
                    Ok(r) => Event::FriendRequest(self, r),
                    Err(e) => {
                        error!("Failed to receive friend request on account {} ({})", self.data.tox_id, e);
                        Event::None
                    }
                }
            }
            _ = Self::handle_incoming_messages(&mut self.incoming_messages, &mut self.chatrooms) => { Event::None }
        }
    }
}
