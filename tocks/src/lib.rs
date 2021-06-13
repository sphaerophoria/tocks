// Seems to trigger incorrectly frequently
#![allow(clippy::needless_lifetimes)]

pub mod contact;

mod account;
mod audio;
mod message_parser;
mod savemanager;
mod storage;

pub use crate::{
    account::AccountId,
    audio::{AudioDevice, FormattedAudio},
    contact::{Friend, Status, User},
    storage::{ChatHandle, ChatLogEntry, ChatMessageId, UserHandle},
};

use anyhow::{Context, Result};
use audio::RepeatingAudioHandle;

use crate::{
    account::{Account, AccountManager},
    audio::AudioManager,
};

use toxcore::ToxId;

use lazy_static::lazy_static;
use log::*;
use platform_dirs::AppDirs;
use futures::{
    channel::mpsc,
    prelude::*,
};
use serde::{Serialize, Deserialize};

lazy_static! {
    pub static ref APP_DIRS: AppDirs = AppDirs::new(Some("tocks"), false).unwrap();
}

// UI things that tocks will need to react to
#[derive(Serialize, Deserialize)]
pub enum TocksUiEvent {
    Close,
    CreateAccount(String /*name*/, String /*password*/),
    AcceptPendingFriend(AccountId, UserHandle),
    BlockUser(AccountId, UserHandle),
    Login(String /* Tox account name */, String /*password*/),
    MessageSent(AccountId, ChatHandle, String /* message */),
    LoadMessages(AccountId, ChatHandle),
    PlaySound(FormattedAudio),
    // Temporary events to test audio playback
    PlaySoundRepeating(FormattedAudio),
    StopRepeatingSound,
    AudioDeviceSelected(AudioDevice),
}

// Things external observers (like the UI) may want to observe
#[derive(Serialize, Deserialize)]
pub enum TocksEvent {
    Error(String),
    AccountListLoaded(Vec<String>),
    AccountLoggedIn(AccountId, UserHandle, ToxId, String),
    FriendAdded(AccountId, Friend),
    FriendRemoved(AccountId, UserHandle),
    BlockedUserAdded(AccountId, User),
    MessagesLoaded(AccountId, ChatHandle, Vec<ChatLogEntry>),
    MessageInserted(AccountId, ChatHandle, ChatLogEntry),
    MessageCompleted(AccountId, ChatHandle, ChatMessageId),
    FriendStatusChanged(AccountId, UserHandle, Status),
    UserNameChanged(AccountId, UserHandle, String),
    AudioDeviceAdded(AudioDevice),
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
    audio_manager: AudioManager,
    ui_event_rx: mpsc::UnboundedReceiver<TocksUiEvent>,
    tocks_event_tx: mpsc::UnboundedSender<TocksEvent>,
    repeating_sound: Option<RepeatingAudioHandle>,
}

impl Tocks {
    pub fn new(
        ui_event_rx: mpsc::UnboundedReceiver<TocksUiEvent>,
        tocks_event_tx: mpsc::UnboundedSender<TocksEvent>,
    ) -> Tocks {
        let mut tocks = Tocks {
            account_manager: AccountManager::new(),
            // FIXME: better error handling
            // FIXME: initialize audiomanager with saved output device
            audio_manager: AudioManager::new().expect("Failed to start audio manager"),
            ui_event_rx,
            tocks_event_tx,
            repeating_sound: None,
        };

        // Intentionally discard errors here. We'll get more errors later that
        // the user can be presented with in the UI
        let _ = std::fs::create_dir_all(&APP_DIRS.data_dir);

        let account_list = account::retrieve_account_list().unwrap_or_default();
        Self::send_tocks_event(
            &tocks.tocks_event_tx,
            TocksEvent::AccountListLoaded(account_list),
        );

        // FIXME: dynamically detect changes and add to outputs
        // FIXME: better error handling
        for device in tocks
            .audio_manager
            .output_devices()
            .expect("Failed to retrieve audio devices")
        {
            Self::send_tocks_event(&tocks.tocks_event_tx, TocksEvent::AudioDeviceAdded(device))
        }

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
                let (account_event_tx, account_event_rx) = mpsc::unbounded();
                let account = Account::from_account_name(name, password, account_event_tx)
                    .context("Failed to create account")?;

                let account_id = self.account_manager.add_account(account, account_event_rx);
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
            TocksUiEvent::AcceptPendingFriend(account_id, user_handle) => {
                let account = self.account_manager.get_mut(&account_id);

                match account {
                    Some(account) => {
                        let friend = account
                            .add_pending_friend(&user_handle)
                            .context("Failed to add pending tox friend")?;

                        Self::send_tocks_event(
                            &self.tocks_event_tx,
                            TocksEvent::FriendStatusChanged(
                                account_id,
                                *friend.id(),
                                *friend.status(),
                            ),
                        );
                    }
                    None => {
                        error!("Account {} not present", account_id);
                    }
                }
            }
            TocksUiEvent::BlockUser(account_id, user_handle) => {
                let account = self.account_manager.get_mut(&account_id);

                match account {
                    Some(account) => {
                        let blocked_user = account
                            .block_user(&user_handle)
                            .context("Failed to reject pending friend")?;

                        Self::send_tocks_event(
                            &self.tocks_event_tx,
                            TocksEvent::FriendRemoved(account_id, user_handle),
                        );

                        Self::send_tocks_event(
                            &self.tocks_event_tx,
                            TocksEvent::BlockedUserAdded(account_id, blocked_user),
                        );
                    }
                    None => {
                        error!("Account {} not present", account_id);
                    }
                }
            }
            TocksUiEvent::Login(account_name, password) => {
                let (account_event_tx, account_event_rx) = mpsc::unbounded();
                let account =
                    Account::from_account_name(account_name.clone(), password, account_event_tx)
                        .with_context(|| format!("Failed to create account {}", account_name))?;

                let account_id = self.account_manager.add_account(account, account_event_rx);
                let account = self.account_manager.get(&account_id).unwrap();

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

                for user in account.blocked_users()? {
                    Self::send_tocks_event(
                        &self.tocks_event_tx,
                        TocksEvent::BlockedUserAdded(account_id, user),
                    );
                }
            }
            TocksUiEvent::MessageSent(account_id, chat_handle, message) => {
                let account = self.account_manager.get_mut(&account_id);

                if let Some(account) = account {
                    let entries =
                        account
                            .send_message(&chat_handle, message)
                            .with_context(|| {
                                format!(
                                    "Failed to send message to {} on account {}",
                                    chat_handle.id(),
                                    account_id.id()
                                )
                            })?;

                    for entry in entries {
                        Self::send_tocks_event(
                            &self.tocks_event_tx,
                            TocksEvent::MessageInserted(account_id, chat_handle, entry),
                        );
                    }
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
            TocksUiEvent::PlaySound(audio) => self.audio_manager.play_formatted_audio(audio),
            TocksUiEvent::PlaySoundRepeating(audio) => {
                self.repeating_sound =
                    Some(self.audio_manager.play_repeating_formatted_audio(audio));
            }
            TocksUiEvent::StopRepeatingSound => {
                self.repeating_sound = None;
            }
            TocksUiEvent::AudioDeviceSelected(device) => {
                self.audio_manager
                    .set_output_device(device)
                    .context("Failed to set audio output device")?;
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
                // Propagate event from lower down
                Self::send_tocks_event(&self.tocks_event_tx, e);
                Ok(false)
            }
            Event::None => Ok(false),
        }
    }

    fn send_tocks_event(tocks_event_tx: &mpsc::UnboundedSender<TocksEvent>, event: TocksEvent) {
        // We don't really care if this fails, who am I to say whether or not an
        // external library wants to service my events
        let _ = tocks_event_tx.unbounded_send(event);
    }

    async fn next_event(&mut self) -> Event {
        let ui_events = &mut self.ui_event_rx;
        let accounts = &mut self.account_manager;

        let event = futures::select! {
            request = ui_events.next().fuse() => {
                match request {
                    Some(request) => Event::Ui(request),
                    None => {
                        error!("Unexpected dropped UI requester, closing app");
                        Event::Ui(TocksUiEvent::Close)
                    },
                }
            },
            _ = self.audio_manager.run().fuse() => unreachable!(),
            event = accounts.run().fuse() => event,
        };

        event
    }
}
