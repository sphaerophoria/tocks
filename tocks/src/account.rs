use crate::{
    contact::{Friend, UserManager},
    storage::{ChatHandle, ChatLogEntry, ChatMessageId, Storage, UserHandle},
    Event, TocksEvent, APP_DIRS,
};

use toxcore::{Event as CoreEvent, FriendRequest, Message, PublicKey, Receipt, Tox, ToxId, Status};

use anyhow::{anyhow, Context, Result};
use futures::FutureExt;
use lazy_static::lazy_static;
use log::*;
use platform_dirs::AppDirs;
use tokio::sync::mpsc;

use std::{
    collections::HashMap,
    fs::{self, File},
    io::Read,
    path::PathBuf,
};

lazy_static! {
    pub static ref TOX_SAVE_DIR: PathBuf = AppDirs::new(Some("tox"), false).unwrap().config_dir;
}

pub(crate) enum AccountEvent {
    FriendRequest(FriendRequest),
    ChatMessageInserted(ChatHandle, ChatLogEntry),
    ChatMessageCompleted(ChatHandle, ChatMessageId),
    FriendStatusChanged(UserHandle, Status),
    None,
}

pub(crate) struct Account {
    tox: Tox,
    user_manager: UserManager,
    storage: Storage,
    outgoing_messages: HashMap<Receipt, (ChatHandle, ChatMessageId)>,
    user_handle: UserHandle,
    public_key: PublicKey,
    tox_id: ToxId,
    name: String,
    toxcore_callback_rx: mpsc::UnboundedReceiver<CoreEvent>,
}

impl Account {
    pub fn create(_name: String) -> Result<Account> {
        warn!("Created accounts are not saved and cannot set their names yet");

        Self::from_builder(Tox::builder()?)
    }

    pub fn from_reader<T: Read>(account_buf: &mut T, password: String) -> Result<Account> {
        let mut account_vec = Vec::new();
        account_buf.read_to_end(&mut account_vec)?;

        if !password.is_empty() {
            return Err(anyhow!("Password support is not implemented"));
        }

        let builder = Tox::builder()?.savedata(toxcore::SaveData::ToxSave(&account_vec));

        Self::from_builder(builder)
    }

    pub fn from_account_name(mut account_name: String, password: String) -> Result<Account> {
        account_name.push_str(".tox");
        let account_file_path = TOX_SAVE_DIR.join(account_name);

        let mut account_file =
            File::open(account_file_path).context("Failed to open tox account file")?;

        Self::from_reader(&mut account_file, password)
    }

    pub fn from_builder(builder: toxcore::ToxBuilder) -> Result<Account> {
        let (toxcore_callback_tx, toxcore_callback_rx) = mpsc::unbounded_channel();

        let mut tox = builder
            .event_callback(move |event| {
                toxcore_callback_tx
                    .send(event)
                    .unwrap_or_else(|_| error!("Failed to propagate incoming message"))
            })
            .build()?;

        let self_public_key = tox.self_public_key();
        let tox_id = tox.self_address();
        let mut name = tox.self_name();

        if name.is_empty() {
            name = self_public_key.to_string();
        }

        let db_name = format!("{}.db", name);
        let storage = Storage::open(APP_DIRS.data_dir.join(&db_name));

        let mut storage = match storage {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to open storage: {}", e);
                Storage::open_ram()?
            }
        };

        let self_user_handle = storage
            .add_user(tox.self_public_key(), tox.self_name())
            .context("Failed to add self to DB")?;

        let mut friends = storage
            .friends()?
            .into_iter()
            .map(|friend| (friend.public_key().clone(), friend))
            .collect::<HashMap<_, _>>();

        let tox_friends = tox.friends().context("Failed to initialize tox friends")?;

        let mut user_manager = UserManager::new();

        for tox_friend in tox_friends {
            let friend = match friends.remove(&tox_friend.public_key()) {
                Some(f) => f,
                None => storage
                    .add_friend(tox_friend.public_key(), tox_friend.name())
                    .context("Failed to add friend to storage")?,
            };

            user_manager.add_friend(friend, tox_friend);
        }

        // Remaining friends need to be added. Assume we've already sent a
        // friend request in the past. Even if we wanted to send again, we don't
        // have the toxid to back it up
        for (_, friend) in friends {
            let tox_friend = tox
                .add_friend_norequest(friend.public_key())
                .context("Failed to add missing tox friend")?;

            user_manager.add_friend(friend, tox_friend);
        }

        Ok(Account {
            tox,
            user_manager,
            toxcore_callback_rx,
            storage,
            outgoing_messages: HashMap::new(),
            user_handle: self_user_handle,
            public_key: self_public_key,
            tox_id,
            name,
        })
    }

    pub fn user_handle(&self) -> &UserHandle {
        &self.user_handle
    }

    pub fn public_key(&self) -> &PublicKey {
        &self.public_key
    }

    pub fn address(&self) -> &ToxId {
        &self.tox_id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn friends(&self) -> impl Iterator<Item = &Friend> {
        self.user_manager.friends()
    }

    pub fn add_friend_publickey(&mut self, friend_key: &PublicKey) -> Result<&Friend> {
        // FIXME: eventually need to sync state with toxcore, re-add from DB etc.
        let tox_friend = self
            .tox
            .add_friend_norequest(&friend_key)
            .context("Failed to add friend by public key")?;

        let public_key = tox_friend.public_key();

        // FIXME: In the future once we support offline friends we should swap
        // the order to do this before adding to toxcore
        let friend = self
            .storage
            .add_friend(public_key, tox_friend.name())
            .context("Failed to add friend to storage")?;

        Ok(self.user_manager.add_friend(friend, tox_friend).0)
    }

    pub fn send_message(
        &mut self,
        chat_handle: &ChatHandle,
        message: String,
    ) -> Result<ChatLogEntry> {
        let message = Message::Normal(message);

        let tox_friend = self.user_manager.tox_friend_by_chat_handle(&chat_handle);

        let mut chat_log_entry = self
            .storage
            .push_message(chat_handle, self.user_handle, message)
            .context("Failed to insert message into storage")?;

        chat_log_entry.set_complete(false);

        self.storage
            .add_unresolved_message(chat_log_entry.id())
            .context("Failed to flag message as un-delivered in storage")?;

        if tox_friend.status() != Status::Offline {
            let receipt = self
                .tox
                .send_message(&tox_friend, chat_log_entry.message())
                .context("Failed to send message to tox friend")?;

            self.outgoing_messages.insert(receipt, (*chat_handle, *chat_log_entry.id()));
        }


        Ok(chat_log_entry)
    }

    // FIXME: In the future this API should support some bounds on which segment
    // of the chat history we want to load, but for now, since no one who uses
    // this will have enough messages for it to matter, we just load them all
    pub fn load_messages(&mut self, chat_handle: &ChatHandle) -> Result<Vec<ChatLogEntry>> {
        self.storage.load_messages(chat_handle)
    }

    pub(crate) async fn run<'a>(&'a mut self) -> Result<AccountEvent> {
        tokio::select! {
            _ = self.tox.run() => { Ok(AccountEvent::None) }
            toxcore_callback = self.toxcore_callback_rx.recv() => {
                match toxcore_callback {
                    Some(CoreEvent::MessageReceived(tox_friend, message)) => {
                        let friend = self.user_manager.friend_by_public_key(&tox_friend.public_key());
                        let chat_log_entry = self.storage.push_message(friend.chat_handle(), *friend.id(), message)
                            .context("Failed to insert incoming message into storage")?;
                        Ok(AccountEvent::ChatMessageInserted(*friend.chat_handle(), chat_log_entry))
                    },
                    Some(CoreEvent::FriendRequest(request)) => {
                        Ok(AccountEvent::FriendRequest(request))
                    },
                    Some(CoreEvent::ReadReceipt(receipt)) => {
                        if let Some((handle, message_id)) = self.outgoing_messages.remove(&receipt) {
                            self.storage.resolve_message(&handle, &message_id)
                                .context("Failed to resolve message")?;

                            Ok(AccountEvent::ChatMessageCompleted(handle, message_id))
                        } else {
                            error!("Received receipt to unknown message");
                            Ok(AccountEvent::None)
                        }

                    },
                    Some(CoreEvent::StatusUpdated(tox_friend)) => {
                        let friend = self.user_manager.friend_by_public_key(&tox_friend.public_key());

                        if *friend.status() == Status::Offline && tox_friend.status() != Status::Offline {
                            let messages = self.storage.unresovled_messages(friend.chat_handle())
                                .context("Failed to retrieve unsent messages")?;

                            for message in messages {
                                let receipt = self.tox.send_message(&tox_friend, message.message())
                                    .context("Failed to send unsent message")?;
                                self.outgoing_messages.insert(receipt, (*friend.chat_handle(), *message.id()));
                            }
                        }

                        friend.set_status(tox_friend.status());
                        Ok(AccountEvent::FriendStatusChanged(*friend.id(), *friend.status()))
                    }
                    None => Ok(AccountEvent::None),
                }
            }
        }
    }
}

#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug)]
pub struct AccountId {
    id: i64,
}

impl AccountId {
    pub fn id(&self) -> i64 {
        self.id
    }
}

impl From<i64> for AccountId {
    fn from(id: i64) -> AccountId {
        AccountId { id }
    }
}

pub(crate) struct AccountManager {
    accounts: HashMap<AccountId, Account>,
    next_account_id: i64,
}

impl AccountManager {
    pub fn new() -> AccountManager {
        AccountManager {
            accounts: HashMap::new(),
            next_account_id: 0,
        }
    }

    pub fn add_account(&mut self, account: Account) -> AccountId {
        let account_id = AccountId {
            id: self.next_account_id,
        };

        self.next_account_id += 1;

        self.accounts.insert(account_id, account);
        account_id
    }

    pub fn get(&self, account_id: &AccountId) -> Option<&Account> {
        self.accounts.get(account_id)
    }

    pub fn get_mut(&mut self, account_id: &AccountId) -> Option<&mut Account> {
        self.accounts.get_mut(account_id)
    }

    pub async fn run(&mut self) -> Event {
        let account_events = if self.accounts.is_empty() {
            // futures::future::select_all is not happy with 0 elements
            futures::future::pending().boxed_local()
        } else {
            let futures = self
                .accounts
                .iter_mut()
                .map(|(id, ac)| async move { (*id, ac.run().await) })
                .map(|fut| fut.boxed());

            futures::future::select_all(futures).boxed()
        };

        // select_all returns a list of all remaining events as the second
        // element. We don't care about the accounts where nothing happened,
        // we'll catch those next time
        match account_events.await.0 {
            (id, Ok(AccountEvent::ChatMessageInserted(chat_handle, m))) => {
                TocksEvent::MessageInserted(id, chat_handle, m).into()
            }
            (id, Ok(AccountEvent::FriendRequest(r))) => {
                TocksEvent::FriendRequestReceived(id, r).into()
            }
            (id, Ok(AccountEvent::ChatMessageCompleted(chat_handle, msg_id))) => {
                TocksEvent::MessageCompleted(id, chat_handle, msg_id).into()
            }
            (id, Ok(AccountEvent::FriendStatusChanged(user_handle, status))) => {
                TocksEvent::FriendStatusChanged(id, user_handle, status).into()
            }
            (_, Ok(AccountEvent::None)) => Event::None,
            (id, Err(e)) => {
                error!("Error in account {:?} handler: {:?}", id, e);
                Event::None
            }
        }
    }
}

pub fn retrieve_account_list() -> Result<Vec<String>> {
    let mut accounts: Vec<String> = fs::read_dir(&*TOX_SAVE_DIR)
        .context("Failed to read tox config dir")?
        .filter(|entry| entry.is_ok())
        .filter_map(|entry| entry.unwrap().file_name().into_string().ok())
        .filter(|item| item.ends_with(".tox"))
        .map(|item| item[..item.len() - 4].to_string())
        .collect();

    accounts.sort();

    Ok(accounts)
}
