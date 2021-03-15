// Seems to trigger incorrectly frequently
#![allow(clippy::needless_lifetimes)]

pub mod contact;
pub mod error;

mod account;
mod chatroom;

pub use crate::{
    account::AccountData,
    chatroom::{ChatEvent, ChatView},
    error::{Error, Result},
};

use crate::{account::Account, contact::FriendData};

use toxcore::{FriendRequest, PublicKey};

use futures::FutureExt;
use log::*;
use platform_dirs::AppDirs;
use tokio::sync::mpsc;

use std::{
    collections::HashMap,
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

type AccountStorage = HashMap<PublicKey, Account>;

pub struct Tocks {
    _appdirs: AppDirs,
    accounts: AccountStorage,
    ui_event_rx: mpsc::UnboundedReceiver<TocksUiEvent>,
    tocks_event_tx: mpsc::UnboundedSender<TocksEvent>,
}

impl Tocks {
    pub fn new(
        ui_event_rx: mpsc::UnboundedReceiver<TocksUiEvent>,
        tocks_event_tx: mpsc::UnboundedSender<TocksEvent>,
    ) -> Tocks {

        let tocks = Tocks {
            _appdirs: AppDirs::new(Some("tocks"), false).unwrap(),
            accounts: HashMap::new(),
            ui_event_rx,
            tocks_event_tx,
        };

        let account_list = account::retrieve_account_list().unwrap_or_default();
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

                let account = Account::create()?;
                let account_data = account.data().clone();
                Self::add_account(&mut self.accounts, account);

                Self::send_tocks_event(
                    &self.tocks_event_tx,
                    TocksEvent::AccountLoggedIn(account_data),
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
                let account = Account::from_account_name(account_name, password)?;
                let account = Self::add_account(&mut self.accounts, account);

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

    fn send_tocks_event(tocks_event_tx: &mpsc::UnboundedSender<TocksEvent>, event: TocksEvent) {
        // We don't really care if this fails, who am I to say whether or not an
        // external library wants to service my events
        let _ = tocks_event_tx.send(event);
    }

    fn add_account(accounts: &mut AccountStorage, account: Account) -> &Account {
        let public_key = account.data().public_key().clone();
        accounts.insert(public_key.clone(), account);
        &accounts[&public_key]
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
