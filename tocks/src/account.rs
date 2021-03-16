use crate::{
    Event,
    contact::Friend,
    storage::{ChatHandle, ChatLogEntry, Storage, UserHandle},
    error::*,
    APP_DIRS,
};

use broadcast::Receiver;
use toxcore::{FriendRequest, Message, Tox, ToxId, PublicKey};

use lazy_static::lazy_static;
use log::*;
use futures::{stream::Stream, Future, FutureExt};
use platform_dirs::AppDirs;
use tokio::sync::broadcast;
use tokio_stream::StreamExt;

use std::{collections::HashMap, fs::{self, File}, io::Read, path::PathBuf, pin::Pin};

pub type Friends = HashMap<UserHandle, Friend>;
pub type ToxFriends = HashMap<ChatHandle, toxcore::Friend>;

lazy_static! {
    pub static ref TOX_SAVE_DIR: PathBuf = AppDirs::new(Some("tox"), false).unwrap().config_dir;
}

pub(crate) enum AccountEvent
{
    FriendRequest(FriendRequest),
    ChatMessageInserted(ChatHandle, ChatLogEntry),
    None,
}
type IncomingMessage = (ChatHandle, UserHandle, Message);
type IncomingMessagesStream = Box<dyn Stream<Item=Result<IncomingMessage>> + Unpin + Send>;

pub(crate) struct Account {
    tox: Tox,
    friends: Friends,
    tox_friends: ToxFriends,
    storage: Storage,
    user_handle: UserHandle,
    public_key: PublicKey,
    tox_id: ToxId,
    name: String,
    friend_requests: broadcast::Receiver<FriendRequest>,
    incoming_messages: Vec<IncomingMessagesStream>,
}

impl Account {
    pub fn create(_name: String) -> Result<Account> {
        warn!("Created accounts are not saved and cannot set their names yet");
        let tox = Tox::builder()?.build()?;

        info!("Created account: {}", tox.self_address());

        Self::from_tox(tox)
    }

    pub fn from_reader<T: Read>(account_buf: &mut T, password: String) -> Result<Account> {
        let mut account_vec = Vec::new();
        account_buf.read_to_end(&mut account_vec)?;

        if !password.is_empty() {
            return Err(Error::Unimplemented("Login with password".into()));
        }

        let tox = Tox::builder()?
            .savedata(toxcore::SaveData::ToxSave(&account_vec))
            .build()?;

        info!("Logged into account: {}", tox.self_address());

        Self::from_tox(tox)
    }

    pub fn from_account_name(mut account_name: String, password: String) -> Result<Account> {
        account_name.push_str(".tox");
        let account_file_path = TOX_SAVE_DIR.join(account_name);
        let mut account_file = File::open(account_file_path)?;
        Self::from_reader(&mut account_file, password)
    }

    pub fn from_tox(mut tox: Tox) -> Result<Account> {
        let friend_requests = tox.friend_requests();
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

        let self_user_handle = storage.add_user(tox.self_public_key(), tox.self_name())?;

        let mut friends = storage.friends()?
            .into_iter()
            .map(|friend| (friend.public_key().clone(), friend))
            .collect::<HashMap<_, _>>();

        let tox_friends = tox.friends()?;

        let mut incoming_messages = Vec::new();
        let mut tox_friend_map = HashMap::new();

        for tox_friend in tox_friends {
            let friend_incoming_messages = tox.incoming_friend_messages(&tox_friend);

            if friends.get(&tox_friend.public_key()).is_none() {
                let friend = storage.add_friend(tox_friend.public_key(), tox_friend.name())?;
                friends.insert(friend.public_key().clone(), friend);
            }

            let friend = &friends[&tox_friend.public_key()];
            let chat_handle = *friend.chat_handle();
            let user_handle = *friend.id();

            let mapped_incoming_messages = Box::new(map_message_receiver(chat_handle, user_handle, friend_incoming_messages));
            let mapped_incoming_messages = mapped_incoming_messages as Box<dyn Stream<Item=_> + Send + Unpin>;

            incoming_messages.push(mapped_incoming_messages);

            tox_friend_map.insert(*friend.chat_handle(), tox_friend);
        }

        let friends = friends
            .into_iter()
            .map(|(_, f)| (*f.id(), f))
            .collect();

        Ok(Account {
            tox,
            friends,
            tox_friends: tox_friend_map,
            friend_requests,
            incoming_messages,
            storage,
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

    pub fn friends(&self) -> &Friends {
        &self.friends
    }

    pub fn add_friend_publickey(&mut self, friend_key: &PublicKey) -> Result<UserHandle> {
        // FIXME: eventually need to sync state with toxcore, re-add from DB etc.
        let tox_friend = self.tox.add_friend_norequest(&friend_key)?;
        let public_key = tox_friend.public_key();

        // FIXME: In the future once we support offline friends we should swap
        // the order to do this before adding to toxcore

        let friend = self.storage.add_friend(public_key, tox_friend.name())?;

        let chat_handle = *friend.chat_handle();
        let user_handle = *friend.id();

        self.friends.insert(*friend.id(), friend);

        let stream = map_message_receiver(chat_handle, user_handle, self.tox.incoming_friend_messages(&self.tox_friends[&chat_handle]));
        let stream = Box::new(stream);

        self.incoming_messages.push(
            Box::new(stream as Box<dyn Stream<Item=_> + Send + Unpin>)
        );

        Ok(user_handle)
    }

    pub fn send_message(&mut self, chat_handle: &ChatHandle, message: String) -> Result<ChatLogEntry> {
        let message = Message::Normal(message);

        // Send message to the DB before we send it to toxcore. On the one hand
        // it kind of sucks to do this synchronously, and also sucks that if you
        // fail to persist the message that it doesn't get sent to your peer.
        //
        // That being said, I think it sucks more if your data doesn't end up in
        // history. I'd rather prioritize that the data integrity is preserved
        // than get the message out a little faster. If this starts to become a
        // problem we can try improving the performance of sqlite, or re-evaluate
        // this decision. Since we are using the storage backed ID
        let chat_log_entry = self.storage.push_message(chat_handle, self.user_handle, message)?;
        let tox_friend = &self.tox_friends[&chat_handle];

        let receipt = self.tox.send_message(&tox_friend, &chat_log_entry.message())?;

        if let Err(e) = self.storage.add_unresolved_receipt(chat_log_entry.id(), &receipt) {
            error!("Failed to add receipt id for outgoing message: {}", e);
        }

        // FIXME: Until we hook up the toxcore API to resolve receipts we just
        // immediately resolve them to prevent our db from filling up with
        // messages that will never be resolved
        self.storage.resolve_receipt(&receipt)?;

        Ok(chat_log_entry)
    }

    // FIXME: In the future this API should support some bounds on which segment
    // of the chat history we want to load, but for now, since no one who uses
    // this will have enough messages for it to matter, we just load them all
    pub fn load_messages(&mut self, chat_handle: &ChatHandle) -> Result<Vec<ChatLogEntry>>
    {
        self.storage.load_messages(chat_handle)
    }

    async fn wait_for_incoming_message(
        incoming_messages: &mut Vec<IncomingMessagesStream>,
    ) -> Result<IncomingMessage> {
        if incoming_messages.is_empty() {
            futures::future::pending::<()>().await;
        }

        let next_incoming_messages = incoming_messages
            .iter_mut()
            .map(|item| async move { item.next().await.unwrap() }.boxed());

        futures::future::select_all(next_incoming_messages).await.0
    }

    pub(crate) async fn run<'a>(&'a mut self) -> Result<AccountEvent> {
        tokio::select! {
            _ = self.tox.run() => { Ok(AccountEvent::None) }
            friend_request = self.friend_requests.recv() => {
                Ok(AccountEvent::FriendRequest(friend_request?))
            }
            result = Self::wait_for_incoming_message(&mut self.incoming_messages) => {
                if let Ok((chat_handle, user_handle, message)) = result {
                    let chat_log_entry = self.storage.push_message(&chat_handle, user_handle, message)?;
                    Ok(AccountEvent::ChatMessageInserted(chat_handle, chat_log_entry))
                }
                else {
                    Ok(AccountEvent::None)
                }
            }
        }
    }
}

#[derive(Copy, Clone, Hash, PartialEq, Eq, Debug)]
pub struct AccountId
{
    id: i64,
}

impl AccountId
{
    pub fn id(&self) -> i64 {
        self.id
    }
}

impl From<i64> for AccountId
{
    fn from(id: i64) -> AccountId {
        AccountId { id }
    }
}

pub(crate) struct AccountManager
{
    accounts: HashMap<AccountId, Account>,
    next_account_id: i64,
}

impl AccountManager
{
    pub fn new() -> AccountManager {
        AccountManager {
            accounts: HashMap::new(),
            next_account_id: 0,
        }
    }

    pub fn add_account(&mut self, account: Account) -> AccountId
    {
        let account_id = AccountId{ id: self.next_account_id };

        self.next_account_id += 1;

        self.accounts.insert(account_id, account);
        account_id
    }

    pub fn get(&self, account_id: &AccountId) -> Option<&Account>
    {
        self.accounts.get(account_id)
    }

    pub fn get_mut(&mut self, account_id: &AccountId) -> Option<&mut Account>
    {
        self.accounts.get_mut(account_id)
    }

    pub async fn run(&mut self) -> Event
    {
        let account_events = if self.accounts.is_empty() {
            // futures::future::select_all is not happy with 0 elements
            futures::future::pending().boxed_local()
        } else {
            let futures = self.accounts
                .iter_mut()
                .map(|(id, ac)| async move { (*id, ac.run().await) })
                .map(|fut| fut.boxed());

            futures::future::select_all(futures).boxed()
        };

        // select_all returns a list of all remaining events as the second
        // element. We don't care about the accounts where nothing happened,
        // we'll catch those next time
        match account_events.await.0 {
            (id, Ok(AccountEvent::ChatMessageInserted(chat_handle, m))) => Event::ChatMessageInserted(id, chat_handle, m),
            (id, Ok(AccountEvent::FriendRequest(r))) => Event::FriendRequest(id, r),
            (_, Ok(AccountEvent::None)) => Event::None,
            (id, Err(e)) => {
                error!("Error in account {:?} handler: {}", id, e);
                Event::None
            }
        }
    }
}

pub fn retrieve_account_list() -> Result<Vec<String>> {
    let mut accounts: Vec<String> = fs::read_dir(&*TOX_SAVE_DIR)?
        .filter(|entry| entry.is_ok())
        .filter_map(|entry| entry.unwrap().file_name().into_string().ok())
        .filter(|item| item.ends_with(".tox"))
        .map(|item| item[..item.len() - 4].to_string())
        .collect();

    accounts.sort();

    Ok(accounts)
}

fn map_message_receiver(chat_handle: ChatHandle, user_handle: UserHandle, receiver: Receiver<Message>) -> impl Stream<Item=Result<IncomingMessage>> + Unpin
{
    let stream = tokio_stream::wrappers::BroadcastStream::new(receiver);
    stream.map(move |res| {
        res.map(|message| {
            (chat_handle, user_handle, message)
        })
        .map_err(|e| Error::from(e))
    })
}
