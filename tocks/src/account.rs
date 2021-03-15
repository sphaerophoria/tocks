use crate::{
    Event,
    chatroom::{ChatRoom, ChatView},
    error::*,
};

use toxcore::{Friend, FriendRequest, Message, Tox, ToxId, PublicKey};

use lazy_static::lazy_static;
use log::*;
use futures::FutureExt;
use platform_dirs::AppDirs;
use tokio::sync::broadcast;

use std::{
    collections::HashMap,
    io::Read,
    fs::{self, File},
    path::PathBuf,
};

pub type Friends = HashMap<PublicKey, Friend>;

lazy_static! {
    pub static ref TOX_SAVE_DIR: PathBuf = AppDirs::new(Some("tox"), false).unwrap().config_dir;
}

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

pub(crate) struct Account {
    tox: Tox,
    friends: Friends,
    chatrooms: HashMap<PublicKey, ChatRoom>,
    friend_requests: broadcast::Receiver<FriendRequest>,
    incoming_messages: HashMap<PublicKey, broadcast::Receiver<Message>>,
    data: AccountData,
}

impl Account {
    pub fn create() -> Result<Account> {
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

    pub fn data(&self) -> &AccountData {
        &self.data
    }

    pub fn friends(&self) -> &Friends {
        &self.friends
    }

    pub fn chatview(&self, key: &PublicKey) -> Option<ChatView> {
        self.chatrooms.get(key).map(|item| item.view())
    }

    pub fn add_friend_publickey(&mut self, friend_key: &PublicKey) -> Result<()> {
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

    pub fn send_message(&mut self, friend: &PublicKey, message: String) -> Result<()> {
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

    pub(crate) async fn run<'a>(&'a mut self) -> Event<'a> {
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
