// Seems to trigger incorrectly frequently
#![allow(clippy::needless_lifetimes)]

pub mod contact;

mod account;
mod storage;

pub use crate::{
    account::AccountId,
    contact::Friend,
    storage::{ChatHandle, ChatLogEntry, ChatMessageId, UserHandle},
};

use anyhow::{Context, Result};

use crate::account::{Account, AccountManager};

use toxcore::{FriendRequest, PublicKey, Status, ToxId};

use lazy_static::lazy_static;
use log::*;
use platform_dirs::AppDirs;
use tokio::sync::mpsc;

lazy_static! {
    pub static ref APP_DIRS: AppDirs = AppDirs::new(Some("tocks"), false).unwrap();
}

// UI things that tocks will need to react to
pub enum TocksUiEvent {
    Close,
    CreateAccount(String /*name*/, String /*password*/),
    AddFriendByPublicKey(AccountId, PublicKey /*friend address*/),
    Login(String /* Tox account name */, String /*password*/),
    MessageSent(AccountId, ChatHandle, String /* message */),
    LoadMessages(AccountId, ChatHandle),
}

// Things external observers (like the UI) may want to observe
pub enum TocksEvent {
    Error(String),
    AccountListLoaded(Vec<String>),
    AccountLoggedIn(AccountId, UserHandle, ToxId, String),
    FriendRequestReceived(AccountId, FriendRequest),
    FriendAdded(AccountId, Friend),
    MessagesLoaded(AccountId, ChatHandle, Vec<ChatLogEntry>),
    MessageInserted(AccountId, ChatHandle, ChatLogEntry),
    MessageCompleted(AccountId, ChatHandle, ChatMessageId),
    FriendStatusChanged(AccountId, UserHandle, Status),
}

// Things that Tocks can handle in it's core iteration loop
enum Event {
    Ui(TocksUiEvent),
    Tocks(TocksEvent),
    None,
}

impl From<TocksEvent> for Event {
    fn from(event: TocksEvent) -> Event {
        Event::Tocks(event)
    }
}

pub struct Tocks {
    account_manager: AccountManager,
    ui_event_rx: mpsc::UnboundedReceiver<TocksUiEvent>,
    tocks_event_tx: mpsc::UnboundedSender<TocksEvent>,
}

impl Tocks {
    pub fn new(
        ui_event_rx: mpsc::UnboundedReceiver<TocksUiEvent>,
        tocks_event_tx: mpsc::UnboundedSender<TocksEvent>,
    ) -> Tocks {
        let tocks = Tocks {
            account_manager: AccountManager::new(),
            ui_event_rx,
            tocks_event_tx,
        };

        // Intentionally discard errors here. We'll get more errors later that
        // the user can be presented with in the UI
        let _ = std::fs::create_dir_all(&APP_DIRS.data_dir);

        let account_list = account::retrieve_account_list().unwrap_or_default();
        Self::send_tocks_event(
            &tocks.tocks_event_tx,
            TocksEvent::AccountListLoaded(account_list),
        );

        tocks
    }

    pub async fn run(&mut self) {
        loop {
            let event = self.next_event().await;

            match self.handle_event(event) {
                Ok(true) => return,
                Ok(false) => (),
                Err(e) => {
                    error!("{:?}", e)
                }
            }
        }
    }

    /// Returns `true` if app should be closed
    fn handle_ui_request(&mut self, event: TocksUiEvent) -> Result<bool> {
        match event {
            TocksUiEvent::Close => {
                return Ok(true);
            }
            TocksUiEvent::CreateAccount(name, password) => {
                if !password.is_empty() {
                    warn!("Account passwords are not yet supported");
                }

                let account = Account::create(name).context("Failed to create account")?;

                let account_id = self.account_manager.add_account(account);
                let account = self.account_manager.get(&account_id).unwrap();

                Self::send_tocks_event(
                    &self.tocks_event_tx,
                    TocksEvent::AccountLoggedIn(
                        account_id,
                        *account.user_handle(),
                        account.address().clone(),
                        account.name().to_string(),
                    ),
                );
            }
            TocksUiEvent::AddFriendByPublicKey(account_id, friend_address) => {
                let account = self.account_manager.get_mut(&account_id);

                match account {
                    Some(account) => {
                        let friend = account
                            .add_friend_publickey(&friend_address)
                            .context("Failed to add tox friend by public key")?;

                        Self::send_tocks_event(
                            &self.tocks_event_tx,
                            TocksEvent::FriendAdded(account_id, friend.clone()),
                        );
                    }
                    None => {
                        error!("Account {} not present", account_id);
                    }
                }
            }
            TocksUiEvent::Login(account_name, password) => {
                let account = Account::from_account_name(account_name.clone(), password)
                    .with_context(|| format!("Failed to create account {}", account_name))?;

                let account_id = self.account_manager.add_account(account);
                let account = &self.account_manager.get(&account_id).unwrap();

                let user_handle = account.user_handle();
                let address = account.address();
                let name = account.name();

                Self::send_tocks_event(
                    &self.tocks_event_tx,
                    TocksEvent::AccountLoggedIn(
                        account_id,
                        *user_handle,
                        address.clone(),
                        name.to_string(),
                    ),
                );

                for friend in account.friends() {
                    Self::send_tocks_event(
                        &self.tocks_event_tx,
                        TocksEvent::FriendAdded(account_id, friend.clone()),
                    );
                }
            }
            TocksUiEvent::MessageSent(account_id, chat_handle, message) => {
                let account = self.account_manager.get_mut(&account_id);

                if let Some(account) = account {
                    let entry = account
                        .send_message(&chat_handle, message)
                        .with_context(|| {
                            format!(
                                "Failed to send message to {} on account {}",
                                chat_handle.id(),
                                account_id.id()
                            )
                        })?;

                    Self::send_tocks_event(
                        &self.tocks_event_tx,
                        TocksEvent::MessageInserted(account_id, chat_handle, entry),
                    );
                } else {
                    error!(
                        "Could not send to {} from {}",
                        chat_handle.id(),
                        account_id.id()
                    );
                }
            }
            TocksUiEvent::LoadMessages(account_id, chat_handle) => {
                let account = self.account_manager.get_mut(&account_id);

                if let Some(account) = account {
                    let messages = account.load_messages(&chat_handle)?;
                    Self::send_tocks_event(
                        &self.tocks_event_tx,
                        TocksEvent::MessagesLoaded(account_id, chat_handle, messages),
                    );
                } else {
                    error!("Could not find account {}", account_id.id());
                }
            }
        };

        Ok(false)
    }

    fn handle_event(&mut self, event: Event) -> Result<bool> {
        match event {
            Event::Ui(request) => self
                .handle_ui_request(request)
                .context("Failed to handle UI request"),
            Event::Tocks(e) => {
                Self::send_tocks_event(&self.tocks_event_tx, e);
                Ok(false)
            }
            Event::None => Ok(false),
        }
    }

    fn send_tocks_event(tocks_event_tx: &mpsc::UnboundedSender<TocksEvent>, event: TocksEvent) {
        // We don't really care if this fails, who am I to say whether or not an
        // external library wants to service my events
        let _ = tocks_event_tx.send(event);
    }

    async fn next_event(&mut self) -> Event {
        let ui_events = &mut self.ui_event_rx;
        let accounts = &mut self.account_manager;

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
            event = accounts.run() => { event }
        };

        event
    }
}
