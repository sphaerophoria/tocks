use crate::{
    audio::AudioFrame,
    calls::{CallEvent, CallManager, CallState},
    contact::{Friend, Status, User, UserManager},
    error::ExitError,
    savemanager::SaveManager,
    storage::{ChatHandle, ChatLogEntry, ChatMessageId, Storage, UserHandle},
    TocksEvent, APP_DIRS,
};

use toxcore::{Event as CoreEvent, Message, PublicKey, Receipt, Status as ToxStatus, Tox, ToxId};

use anyhow::{anyhow, Context, Error, Result};
use fslock::LockFile;
use futures::{channel::mpsc, prelude::*};
use lazy_static::lazy_static;
use log::*;
use platform_dirs::AppDirs;
use serde::{Deserialize, Serialize};

use std::{collections::HashMap, fmt, fs, io::ErrorKind, path::PathBuf};

lazy_static! {
    pub static ref TOX_SAVE_DIR: PathBuf = AppDirs::new(Some("tox"), false).unwrap().config_dir;
}

#[derive(Debug)]
pub(crate) enum AccountEvent {
    FriendAdded(Friend),
    ChatMessageInserted(ChatHandle, ChatLogEntry),
    ChatMessageCompleted(ChatHandle, ChatMessageId),
    FriendStatusChanged(UserHandle, Status),
    UserNameChanged(UserHandle, String),
    CallStateChanged(ChatHandle, CallState),
    AudioDataReceived(ChatHandle, AudioFrame),
}

impl From<(AccountId, AccountEvent)> for TocksEvent {
    fn from(v: (AccountId, AccountEvent)) -> TocksEvent {
        match v.1 {
            AccountEvent::FriendAdded(f) => TocksEvent::FriendAdded(v.0, f),
            AccountEvent::ChatMessageInserted(chat, entry) => {
                TocksEvent::MessageInserted(v.0, chat, entry)
            }
            AccountEvent::ChatMessageCompleted(chat, id) => {
                TocksEvent::MessageCompleted(v.0, chat, id)
            }
            AccountEvent::FriendStatusChanged(user, status) => {
                TocksEvent::FriendStatusChanged(v.0, user, status)
            }
            AccountEvent::UserNameChanged(user, name) => {
                TocksEvent::UserNameChanged(v.0, user, name)
            }
            AccountEvent::CallStateChanged(chat, call_state) => {
                TocksEvent::ChatCallStateChanged(v.0, chat, call_state)
            }
            AccountEvent::AudioDataReceived(chat, frame) => {
                TocksEvent::AudioDataReceived(v.0, chat, frame)
            }
        }
    }
}

pub(crate) struct Account {
    _account_lock: LockFile,
    tox: Tox,
    save_manager: SaveManager,
    user_manager: UserManager,
    call_manager: CallManager,
    storage: Storage,
    outgoing_messages: HashMap<Receipt, (ChatHandle, ChatMessageId)>,
    user_handle: UserHandle,
    public_key: PublicKey,
    tox_id: ToxId,
    name: String,
    toxcore_callback_rx: mpsc::UnboundedReceiver<CoreEvent>,
    account_event_tx: mpsc::UnboundedSender<AccountEvent>,
}

impl Account {
    pub fn from_account_name(
        account_name: String,
        password: String,
        account_event_tx: mpsc::UnboundedSender<AccountEvent>,
    ) -> Result<Account> {
        let account_lock = lock_account(account_name.clone())?;

        let save_manager = create_save_manager(account_name.clone(), &password)?;
        let (mut tox, toxcore_callback_rx) = create_tox(save_manager.load())?;

        let self_public_key = tox.self_public_key();
        let tox_id = tox.self_address();
        let mut name = tox.self_name();

        if name.is_empty() {
            tox.self_set_name(&account_name)
                .context("Failed to initialize account name")?;
            name = tox.self_name();
        }

        let mut storage = create_storage(&account_name, &tox.self_public_key(), &tox.self_name())?;

        let mut user_manager = UserManager::new();

        initialize_friend_lists(&mut storage, &mut tox, &mut user_manager)?;

        // After initializing our friends list our toxcore state could have changed
        save_manager.save(&tox.get_savedata())?;

        let self_user_handle = storage.self_user_handle();

        Ok(Account {
            _account_lock: account_lock,
            tox,
            save_manager,
            user_manager,
            call_manager: CallManager::new(),
            toxcore_callback_rx,
            storage,
            outgoing_messages: HashMap::new(),
            user_handle: self_user_handle,
            public_key: self_public_key,
            tox_id,
            name,
            account_event_tx,
        })
    }

    pub fn user_handle(&self) -> &UserHandle {
        &self.user_handle
    }

    #[allow(dead_code)]
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

    pub fn blocked_users(&self) -> Result<impl Iterator<Item = User>> {
        Ok(self
            .storage
            .blocked_users()
            .context("Failed to retrieve blocked users")?
            .into_iter())
    }

    pub fn add_pending_friend(&mut self, friend_id: &UserHandle) -> Result<&Friend> {
        let bundle = self.user_manager.friend_by_user_handle(&friend_id);
        let friend = &mut bundle.friend;

        if *friend.status() != Status::Pending {
            return Ok(friend);
        }

        bundle.tox_friend = Some(
            self.tox
                .add_friend_norequest(friend.public_key())
                .context("Failed to add friend by public key")?,
        );

        friend.set_status(Status::Offline);

        self.storage
            .resolve_pending_friend_request(friend_id)
            .context("Failed to save pending friend state to DB")?;

        self.save_manager
            .save(&self.tox.get_savedata())
            .context("Failed to save tox data after adding friend")?;

        Ok(friend)
    }

    pub fn request_friend(&mut self, tox_id: ToxId, message: String) -> Result<Friend> {
        let name = tox_id.to_string();
        let tox_friend = self
            .tox
            .add_friend(tox_id, message)
            .context("Failed to add friend")?;

        let friend = self
            .storage
            .add_friend(tox_friend.public_key(), name)
            .context("Failed to add friend to DB")?;

        self.user_manager.add_friend(friend.clone(), tox_friend);

        self.save_manager
            .save(&self.tox.get_savedata())
            .context("Failed to save tox data after adding friend")?;

        Ok(friend)
    }

    pub fn block_user(&mut self, user_id: &UserHandle) -> Result<User> {
        let (friend, user) = {
            let friend_bundle = self.user_manager.friend_by_user_handle(&user_id);
            let tox_friend = &friend_bundle.tox_friend;

            if let Some(tox_friend) = tox_friend {
                // In order to block an accepted friend we need to support friend
                // removal in toxcore
                self.tox
                    .remove_friend(tox_friend)
                    .context("Failed to remove tox friend")?;

                self.save_manager
                    .save(&self.tox.get_savedata())
                    .context("Failed to save tox profile after friend removal")?;
            }

            friend_bundle.tox_friend = None;

            let user = self
                .storage
                .block_user(user_id)
                .context("Failed to remove user from DB")?;

            (friend_bundle.friend.clone(), user)
        };

        self.user_manager.remove_friend(friend);

        Ok(user)
    }

    pub fn purge_user(&mut self, user_id: &UserHandle) -> Result<()> {
        // Re-use block user code since we want basically the same behavior
        self.block_user(user_id)
            .context("Failed to block user before purging")?;

        self.storage.purge_user(user_id)?;

        Ok(())
    }

    pub fn send_message(
        &mut self,
        chat_handle: &ChatHandle,
        message: String,
    ) -> Result<Vec<ChatLogEntry>> {
        let messages = crate::message_parser::parse(message, self.tox.max_message_length())
            .context("Failed to parse input message")?;

        let tox_friend = self
            .user_manager
            .friend_by_chat_handle(&chat_handle)
            .tox_friend
            .as_ref();

        if tox_friend.is_none() {
            return Err(anyhow!("Cannot send message to unaccepted friend"));
        }

        let tox_friend = tox_friend.unwrap();

        let mut ret = Vec::new();

        for message in messages {
            // Attempt to send the message to toxcore first. This ensures that we do
            // not store a message in the DB that is not sendable
            let receipt = if tox_friend.status() != ToxStatus::Offline {
                Some(
                    self.tox
                        .send_message(&tox_friend, &message)
                        .context("Failed to send message to tox friend")?,
                )
            } else {
                None
            };

            let mut chat_log_entry = self
                .storage
                .push_message(chat_handle, self.user_handle, message)
                .context("Failed to insert message into storage")?;

            chat_log_entry.set_complete(false);

            self.storage
                .add_unresolved_message(chat_log_entry.id())
                .context("Failed to flag message as un-delivered in storage")?;

            if let Some(receipt) = receipt {
                self.outgoing_messages
                    .insert(receipt, (*chat_handle, *chat_log_entry.id()));
            }

            ret.push(chat_log_entry);
        }

        Ok(ret)
    }

    // FIXME: In the future this API should support some bounds on which segment
    // of the chat history we want to load, but for now, since no one who uses
    // this will have enough messages for it to matter, we just load them all
    pub fn load_messages(&mut self, chat_handle: &ChatHandle) -> Result<Vec<ChatLogEntry>> {
        self.storage.load_messages(chat_handle)
    }

    pub fn join_call(&mut self, chat_handle: &ChatHandle) -> Result<CallState> {
        let initial_state = self.call_manager.call_state(chat_handle);
        match initial_state {
            CallState::Incoming => {
                self.call_manager
                    .accept_call(chat_handle)
                    .context("Failed to accept call")?;
            }
            CallState::Active | CallState::Outgoing => (),
            CallState::Idle => {
                let core_friend = self
                    .user_manager
                    .friend_by_chat_handle(chat_handle)
                    .tox_friend
                    .as_ref();
                let core_friend =
                    core_friend.ok_or_else(|| anyhow!("Cannot join call with offline friend"))?;

                let outgoing_call = self
                    .tox
                    .call_friend(&core_friend)
                    .context("Failed to initiate call with friend")?;

                self.call_manager.outgoing_call(*chat_handle, outgoing_call);
            }
        }

        Ok(self.call_manager.call_state(chat_handle))
    }

    pub fn leave_call(&mut self, chat_handle: &ChatHandle) {
        self.call_manager.drop_call(chat_handle);
    }

    pub fn send_audio_frame(&mut self, frame: AudioFrame) -> Result<()> {
        self.call_manager.send_audio_frame(frame)
    }

    fn handle_toxcore_event(&mut self, event: CoreEvent) -> Result<()> {
        match event {
            CoreEvent::MessageReceived(tox_friend, message) => {
                let friend = self
                    .user_manager
                    .friend_by_public_key(&tox_friend.public_key());
                let chat_log_entry = self
                    .storage
                    .push_message(friend.chat_handle(), *friend.id(), message)
                    .context("Failed to insert incoming message into storage")?;
                self.account_event_tx
                    .unbounded_send(AccountEvent::ChatMessageInserted(
                        *friend.chat_handle(),
                        chat_log_entry,
                    ))
                    .context("Failed to propagate received message")?;
            }
            CoreEvent::FriendRequest(request) => {
                // FIXME: reject incoming request if the user is blocked

                let friend: Friend = self
                    .storage
                    .add_pending_friend(request.public_key)
                    .context("Failed to add friend_request to DB")?;
                let chat_log_entry = self
                    .storage
                    .push_message(
                        friend.chat_handle(),
                        *friend.id(),
                        Message::Normal(request.message),
                    )
                    .context("Failed to write friend request message to storage")?;
                self.user_manager.add_pending_friend(friend.clone());
                self.account_event_tx
                    .unbounded_send(AccountEvent::FriendAdded(friend.clone()))
                    .context("Failed to propagate friend request")?;
                self.account_event_tx
                    .unbounded_send(AccountEvent::ChatMessageInserted(
                        *friend.chat_handle(),
                        chat_log_entry,
                    ))
                    .context("Failed to propagate friend request message")?;
            }
            CoreEvent::ReadReceipt(receipt) => {
                if let Some((handle, message_id)) = self.outgoing_messages.remove(&receipt) {
                    self.storage
                        .resolve_message(&handle, &message_id)
                        .context("Failed to resolve message")?;

                    self.account_event_tx
                        .unbounded_send(AccountEvent::ChatMessageCompleted(handle, message_id))
                        .context("Failed to propagate message completion")?;
                } else {
                    error!("Received receipt to unknown message");
                }
            }
            CoreEvent::StatusUpdated(tox_friend) => {
                let friend = self
                    .user_manager
                    .friend_by_public_key(&tox_friend.public_key());

                if *friend.status() == Status::Offline && tox_friend.status() != ToxStatus::Offline
                {
                    let messages = self
                        .storage
                        .unresovled_messages(friend.chat_handle())
                        .context("Failed to retrieve unsent messages")?;

                    for message in messages {
                        let receipt = self
                            .tox
                            .send_message(&tox_friend, message.message())
                            .context("Failed to send unsent message")?;
                        self.outgoing_messages
                            .insert(receipt, (*friend.chat_handle(), *message.id()));
                    }
                }

                friend.set_status(Status::from(tox_friend.status()));
                self.account_event_tx
                    .unbounded_send(AccountEvent::FriendStatusChanged(
                        *friend.id(),
                        *friend.status(),
                    ))
                    .context("Failed to propagate status change")?;
            }
            CoreEvent::NameUpdated(tox_friend) => {
                let friend = self
                    .user_manager
                    .friend_by_public_key(&tox_friend.public_key());

                friend.set_name(tox_friend.name());

                if let Err(e) = self.storage.update_user_name(friend.id(), friend.name()) {
                    error!("Failed to update user name in storage: {}", e);
                }

                if let Err(e) = self.save_manager.save(&self.tox.get_savedata()) {
                    error!("Failed to update tox save for user name change: {}", e);
                }

                self.account_event_tx
                    .unbounded_send(AccountEvent::UserNameChanged(
                        *friend.id(),
                        friend.name().to_string(),
                    ))
                    .context("Failed to propagate name change")?;
            }
            CoreEvent::IncomingCall(call) => {
                info!("Incoming call from {}", call.friend().name());

                let friend = self
                    .user_manager
                    .friend_by_public_key(&call.friend().public_key());

                self.call_manager.incoming_call(*friend.chat_handle(), call);

                self.account_event_tx
                    .unbounded_send(AccountEvent::CallStateChanged(
                        *friend.chat_handle(),
                        CallState::Incoming,
                    ))
                    .context("Failed to propagate incoming call")?;
            }
        }

        Ok(())
    }

    fn handle_call_event(&mut self, event: CallEvent) -> Result<()> {
        match event {
            CallEvent::CallEnded(chat) => {
                self.account_event_tx
                    .unbounded_send(AccountEvent::CallStateChanged(chat, CallState::Idle))
                    .context("Failed to propagate ended call")?;
            }
            CallEvent::AudioReceived(chat, frame) => {
                self.account_event_tx
                    .unbounded_send(AccountEvent::AudioDataReceived(chat, frame))
                    .context("Failed to propagate audio data")?;
            }
            CallEvent::CallAccepted(chat) => {
                self.account_event_tx
                    .unbounded_send(AccountEvent::CallStateChanged(chat, CallState::Active))
                    .context("Failed to propagate ended call")?;
            }
        }

        Ok(())
    }

    pub(crate) async fn run(&mut self) -> Result<()> {
        loop {
            futures::select! {
                _ = self.tox.run().fuse() => {
                    Err(ExitError::Ungraceful)
                        .context("Tox account unexpectedly stopped")?;
                },
                toxcore_callback = self.toxcore_callback_rx.next().fuse() => {
                    let event = toxcore_callback
                        .context(ExitError::Ungraceful)
                        .context("Toxcore callback channel unexpectedly dropped")?;

                    if let Err(e) = self.handle_toxcore_event(event) {
                        error!("Failed to handle toxcore event: {}", e)
                    }
                }
                event = self.call_manager.run().fuse() => {
                    if let Err(e) = self.handle_call_event(event) {
                        error!("Failed to handle call event: {}", e)
                    }
                }
            }
        }
    }
}

impl Drop for Account {
    fn drop(&mut self) {
        if let Err(e) = self.save_manager.save(&self.tox.get_savedata()) {
            error!("Failed to save tox save on shutdown: {}", e);
        }
    }
}

#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AccountId {
    id: i64,
}

impl AccountId {
    pub fn id(&self) -> i64 {
        self.id
    }
}

impl fmt::Display for AccountId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.id)
    }
}

impl From<i64> for AccountId {
    fn from(id: i64) -> AccountId {
        AccountId { id }
    }
}

struct AccountBundle {
    account: Account,
    account_events: mpsc::UnboundedReceiver<AccountEvent>,
}

pub(crate) struct AccountManager {
    accounts: HashMap<AccountId, AccountBundle>,
    next_account_id: i64,
}

impl AccountManager {
    pub fn new() -> AccountManager {
        AccountManager {
            accounts: HashMap::new(),
            next_account_id: 0,
        }
    }

    pub fn add_account(
        &mut self,
        account: Account,
        account_events: mpsc::UnboundedReceiver<AccountEvent>,
    ) -> AccountId {
        let account_id = AccountId {
            id: self.next_account_id,
        };

        self.next_account_id += 1;

        self.accounts.insert(
            account_id,
            AccountBundle {
                account,
                account_events,
            },
        );
        account_id
    }

    pub fn accounts_mut(&mut self) -> impl ExactSizeIterator<Item = &mut Account> {
        self.accounts.iter_mut().map(|bundle| &mut bundle.1.account)
    }

    pub fn get(&self, account_id: &AccountId) -> Option<&Account> {
        self.accounts.get(account_id).map(|bundle| &bundle.account)
    }

    pub fn get_mut(&mut self, account_id: &AccountId) -> Option<&mut Account> {
        self.accounts
            .get_mut(account_id)
            .map(|bundle| &mut bundle.account)
    }

    async fn run_account_bundle(
        id: AccountId,
        bundle: &mut AccountBundle,
    ) -> Option<(AccountId, AccountEvent)> {
        futures::select! {
            _  = bundle.account.run().fuse() => None,
            event = bundle.account_events.next().fuse() => {
                event.map(|event| (id, event))
            }
        }
    }

    pub async fn run(&mut self) -> Result<TocksEvent> {
        let account_events = if self.accounts.is_empty() {
            // futures::future::select_all is not happy with 0 elements
            futures::future::pending().boxed_local()
        } else {
            let futures = self
                .accounts
                .iter_mut()
                .map(|(id, bundle)| Self::run_account_bundle(*id, bundle))
                .map(|fut| fut.boxed());

            futures::future::select_all(futures).boxed()
        };

        // select_all returns a list of all remaining events as the second
        // element. We don't care about the accounts where nothing happened,
        // we'll catch those next time
        Ok(account_events
            .await
            .0
            .context(ExitError::Ungraceful)
            .context("All accounts unexpectedly dropped")?
            .into())
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

fn create_save_manager(account_name: String, password: &str) -> Result<SaveManager> {
    let mut account_file = account_name;
    account_file.push_str(".tox");
    let account_file_path = TOX_SAVE_DIR.join(account_file);

    let save_manager = if password.is_empty() {
        SaveManager::new_unencrypted(account_file_path)
    } else {
        SaveManager::new_with_password(account_file_path, password)
            .context("Failed to create save manager")?
    };

    Ok(save_manager)
}

fn handle_savedata_failure(savedata: Result<Vec<u8>>) -> Result<Option<Vec<u8>>> {
    match savedata {
        Ok(d) => Ok(Some(d)),
        Err(e) => {
            if let Some(io_err) = e.downcast_ref::<std::io::Error>() {
                if io_err.kind() == ErrorKind::NotFound {
                    return Ok(None);
                }
            }
            Err(e).context("Failed to load tox save")
        }
    }
}

fn create_tox(
    savedata: Result<Vec<u8>>,
) -> Result<(Tox, mpsc::UnboundedReceiver<toxcore::Event>), Error> {
    let (toxcore_callback_tx, toxcore_callback_rx) = mpsc::unbounded();

    let builder = Tox::builder()?;

    let savedata = handle_savedata_failure(savedata)?;

    let builder = match savedata {
        Some(d) => builder.savedata(toxcore::SaveData::ToxSave(d)),
        None => builder,
    };

    let tox = builder
        .event_callback(move |event| {
            toxcore_callback_tx
                .unbounded_send(event)
                .unwrap_or_else(|_| error!("Failed to propagate incoming message"))
        })
        .log(true)
        .build()?;

    Ok((tox, toxcore_callback_rx))
}

fn create_storage(account_name: &str, self_pk: &PublicKey, current_name: &str) -> Result<Storage> {
    let db_name = format!("{}.db", account_name);
    let storage = Storage::open(APP_DIRS.data_dir.join(&db_name), self_pk, current_name);

    let storage = match storage {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to open storage: {}", e);
            Storage::open_ram(self_pk, current_name).context("Failed to open ram DB")?
        }
    };

    Ok(storage)
}

/// Initialize friend lists ensuring consistency between DB state and toxcore
/// state.
///
/// The goals here are as follows
///    1. Ensure all existing tox friends are in our DB
///        * This is likely to get out of sync when users use multiple tox clients
///    2. Ensure any friends the DB thinks we should have are in toxcore
///    3. Add all friends to our runtime UserManager
///
/// Note that this falls over if a user switches to another tox client and
/// removes a friend. That friend will be re-added because we do not know
/// if we failed to add the friend to toxcore in a previous tocks run or if
/// the user intentionally removed the friend from another tox client
fn initialize_friend_lists(
    storage: &mut Storage,
    tox: &mut Tox,
    user_manager: &mut UserManager,
) -> Result<()> {
    let mut friends = storage
        .friends()?
        .into_iter()
        .map(|friend| (friend.public_key().clone(), friend))
        .collect::<HashMap<_, _>>();

    let tox_friends = tox.friends().context("Failed to initialize tox friends")?;

    for tox_friend in tox_friends {
        let mut friend = match friends.remove(&tox_friend.public_key()) {
            Some(f) => f,
            None => storage
                .add_friend(tox_friend.public_key(), tox_friend.name())
                .context("Failed to add friend to storage")?,
        };

        if *friend.status() == Status::Pending {
            friend.set_status(Status::Offline);
            storage
                .resolve_pending_friend_request(friend.id())
                .context("Failed to remove pending friend request from storage")?;
        }

        if friend.name() != tox_friend.name() {
            friend.set_name(tox_friend.name());
            storage
                .update_user_name(friend.id(), friend.name())
                .context("Failed to update user name")?;
        }

        user_manager.add_friend(friend, tox_friend);
    }

    // Remaining friends need to be added. Assume we've already sent a
    // friend request in the past. Even if we wanted to send again, we don't
    // have the toxid to back it up
    for (_, friend) in friends {
        if *friend.status() != Status::Pending {
            let tox_friend = tox
                .add_friend_norequest(friend.public_key())
                .context("Failed to add missing tox friend")?;

            user_manager.add_friend(friend, tox_friend);
        } else {
            user_manager.add_pending_friend(friend);
        }
    }

    Ok(())
}

fn lock_account(mut account_name: String) -> Result<LockFile> {
    account_name.push_str(".lock");

    let lock_path = APP_DIRS.data_dir.join(account_name);

    let mut lock_file = LockFile::open(&lock_path).context("Failed to open lock file")?;

    let lock_success = lock_file.try_lock().context("Io error on lock file")?;

    if !lock_success {
        return Err(anyhow!("Failed to lock account"));
    }

    Ok(lock_file)
}
