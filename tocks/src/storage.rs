use crate::contact::{Friend, Status};

use toxcore::{Message, PublicKey};

use anyhow::{anyhow, Context, Error, Result};
use chrono::{DateTime, Utc};
use rusqlite::{params, types::ValueRef, Connection, OptionalExtension, Transaction, NO_PARAMS};

use std::{fmt, path::Path};

const SELF_USER_ID: i64 = 0;

// Wrapper around sqlite message table id
#[derive(PartialEq, Eq, PartialOrd, Ord, Copy, Clone, Debug)]
pub struct ChatMessageId {
    msg_id: i64,
}

impl fmt::Display for ChatMessageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.msg_id)
    }
}

// NOTE: This is written to the DB, so if the meanings of these values are
// changed you may have data consistency issues
#[derive(Debug)]
pub struct ChatLogEntry {
    id: ChatMessageId,
    sender: UserHandle,
    message: Message,
    timestamp: DateTime<Utc>,
    complete: bool,
}

impl ChatLogEntry {
    pub fn id(&self) -> &ChatMessageId {
        &self.id
    }

    pub fn sender(&self) -> &UserHandle {
        &self.sender
    }

    pub fn message(&self) -> &Message {
        &self.message
    }

    pub fn timestamp(&self) -> &DateTime<Utc> {
        &self.timestamp
    }

    pub fn complete(&self) -> bool {
        self.complete
    }

    pub fn set_complete(&mut self, complete: bool) {
        self.complete = complete;
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChatHandle {
    chat_id: i64,
}

impl ChatHandle {
    pub fn id(&self) -> i64 {
        self.chat_id
    }
}

impl From<i64> for ChatHandle {
    fn from(id: i64) -> Self {
        Self { chat_id: id }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UserHandle {
    user_id: i64,
}

impl fmt::Display for UserHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.user_id)
    }
}

impl UserHandle {
    pub fn id(&self) -> i64 {
        self.user_id
    }
}

impl From<i64> for UserHandle {
    fn from(id: i64) -> Self {
        Self { user_id: id }
    }
}

pub struct UnsentMessage {
    id: ChatMessageId,
    message: Message,
}

impl UnsentMessage {
    pub fn id(&self) -> &ChatMessageId {
        &self.id
    }

    pub fn message(&self) -> &Message {
        &self.message
    }
}

pub(crate) struct Storage {
    connection: Connection,
}

impl Storage {
    pub fn open<P: AsRef<Path>>(path: P, self_pk: &PublicKey, self_name: &str) -> Result<Storage> {
        let mut connection = Connection::open(&path)
            .with_context(|| format!("Failed to open db at {}", path.as_ref().to_string_lossy()))?;

        initialize_db(&mut connection, self_pk, self_name)?;

        Ok(Storage { connection })
    }

    pub fn open_ram(self_pk: &PublicKey, self_name: &str) -> Result<Storage> {
        let mut connection =
            Connection::open_in_memory().context("Failed to open sqlite db in ram")?;

        initialize_db(&mut connection, self_pk, self_name)?;
        Ok(Storage { connection })
    }

    pub fn self_user_handle(&self) -> UserHandle {
        UserHandle {
            user_id: SELF_USER_ID,
        }
    }

    pub fn friends(&self) -> Result<Vec<Friend>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT chat_id, friends.user_id, users.public_key, users.name, pending_friends.id \
                FROM friends \
                LEFT JOIN users ON friends.user_id = users.id \
                LEFT JOIN pending_friends ON friends.user_id = pending_friends.user_id",
            )
            .context("Failed to prepare statement to retrieve friends from DB")?;

        let query_results = statement
            .query_map(NO_PARAMS, |row| {
                let chat_handle = ChatHandle {
                    chat_id: row.get(0)?,
                };
                let user_handle = UserHandle {
                    user_id: row.get(1)?,
                };
                let public_key_bytes: Vec<u8> = row.get(2)?;
                let name: String = row.get(3)?;

                let pending: bool = row.get_raw(4) != ValueRef::Null;

                Ok((chat_handle, user_handle, public_key_bytes, name, pending))
            })
            .context("Failed to map friend list response")?;

        query_results
            .into_iter()
            .filter_map(std::result::Result::ok)
            .map(
                |(chat_handle, user_handle, public_key_bytes, name, pending)| {
                    let status = if pending {
                        Status::Pending
                    } else {
                        Status::Offline
                    };
                    Ok(Friend::new(
                        user_handle,
                        chat_handle,
                        PublicKey::from_bytes(public_key_bytes)?,
                        name,
                        status,
                    ))
                },
            )
            .collect::<Result<Vec<Friend>>>()
            .context("Failed to convert DB friends")
    }

    pub fn add_friend(&mut self, public_key: PublicKey, name: String) -> Result<Friend> {
        let transaction = self.connection.transaction()?;

        let friend = Self::add_friend_transaction(&transaction, public_key, name)?;

        transaction.commit()?;

        Ok(friend)
    }

    pub fn add_pending_friend(&mut self, public_key: PublicKey) -> Result<Friend> {
        let transaction = self.connection.transaction()?;

        let name = public_key.to_string();
        let mut friend = Self::add_friend_transaction(&transaction, public_key, name)?;

        transaction
            .execute(
                "INSERT INTO pending_friends (user_id) VALUES (?1)",
                params![friend.id().id()],
            )
            .context("Failed to insert into pending friend")?;

        transaction.commit()?;

        friend.set_status(Status::Pending);

        Ok(friend)
    }

    fn add_friend_transaction(
        transaction: &Transaction,
        public_key: PublicKey,
        name: String,
    ) -> Result<Friend> {
        let user_id = Self::add_user_transaction(&transaction, &public_key, &name)?;

        let existing_chat_id = transaction
            .query_row(
                "SELECT chat_id FROM friends WHERE user_id = ?1",
                params![user_id.id()],
                |row| {
                    let chat_id: i64 = row.get(0)?;
                    Ok(chat_id)
                },
            )
            .optional()
            .context("Failed to check if friend already exists in DB")?;

        let chat_id = match existing_chat_id {
            Some(id) => id,
            None => {
                transaction
                    .execute("INSERT INTO chats DEFAULT VALUES", NO_PARAMS)
                    .context("Failed to add chat to DB")?;

                let chat_id = transaction.last_insert_rowid();

                transaction
                    .execute(
                        "INSERT INTO friends (user_id, chat_id) VALUES (?1, ?2)",
                        params![user_id.id(), chat_id],
                    )
                    .context("Failed to add friend to DB")?;

                chat_id
            }
        };

        Ok(Friend::new(
            user_id,
            ChatHandle { chat_id },
            public_key,
            name,
            Status::Offline,
        ))
    }

    // Moved away from using this to add self to the DB but it will be useful to
    // keep around for when we add group support
    #[allow(dead_code)]
    pub fn add_user(&mut self, public_key: PublicKey, name: String) -> Result<UserHandle> {
        let transaction = self.connection.transaction()?;
        let handle = Self::add_user_transaction(&transaction, &public_key, &name)?;
        transaction.commit()?;
        Ok(handle)
    }

    fn add_user_transaction(
        transaction: &Transaction,
        public_key: &PublicKey,
        name: &str,
    ) -> Result<UserHandle> {
        // Check if user is already in users table
        let user_id = transaction
            .query_row(
                "SELECT id FROM users where public_key = ?1",
                params![public_key.as_bytes()],
                |row| {
                    let id: i64 = row.get(0)?;
                    Ok(id)
                },
            )
            .optional()
            .context("Failed to retrieve user from DB")?;

        let user_id = match user_id {
            Some(id) => {
                transaction
                    .execute(
                        "UPDATE users SET name = ?2 WHERE id = ?1",
                        params![id, name],
                    )
                    .context("Failed to update user name")?;
                id
            }
            None => {
                transaction
                    .execute(
                        "INSERT INTO users (public_key, name) VALUES (?1, ?2)",
                        params![public_key.as_bytes(), name],
                    )
                    .context("Failed to add user to DB")?;
                transaction.last_insert_rowid()
            }
        };

        Ok(UserHandle { user_id })
    }

    pub fn resolve_pending_friend_request(&mut self, user_handle: &UserHandle) -> Result<()> {
        self.connection
            .execute(
                "DELETE FROM pending_friends WHERE user_id = ?1",
                params![user_handle.id()],
            )
            .context("Failed to remove user from pending friends table")?;

        Ok(())
    }

    pub fn push_message(
        &mut self,
        chat: &ChatHandle,
        sender: UserHandle,
        message: Message,
    ) -> Result<ChatLogEntry> {
        let timestamp = Utc::now();

        let (message_str, is_action) = match &message {
            Message::Action(s) => (s, true),
            Message::Normal(s) => (s, false),
        };

        let transaction = self.connection.transaction()?;

        transaction
            .execute(
                "INSERT INTO messages (chat_id, sender_id, timestamp) \
                VALUES (?1, ?2, ?3)",
                params![chat.chat_id, sender.user_id, timestamp],
            )
            .context("Failed to insert message into messages table")?;

        let id = ChatMessageId {
            msg_id: transaction.last_insert_rowid(),
        };

        transaction
            .execute(
                "INSERT INTO text_messages (message_id, message, action) \
                VALUES (?1, ?2, ?3)",
                params![id.msg_id, message_str, is_action],
            )
            .context("Failed to insert message into text_messages table")?;

        transaction.commit()?;

        Ok(ChatLogEntry {
            id,
            sender,
            message,
            timestamp,
            // Default to completed, if the caller wants to deal with receipts
            // they can update this once the receipt is injected into storage
            complete: true,
        })
    }

    pub fn load_messages(&mut self, chat: &ChatHandle) -> Result<Vec<ChatLogEntry>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT messages.id, sender_id, timestamp, message, action, pending_messages.id \
                FROM messages \
                LEFT JOIN text_messages ON messages.id = text_messages.message_id \
                LEFT JOIN pending_messages ON messages.id = pending_messages.message_id \
                WHERE chat_id = ?1",
            )
            .context("Failed to prepare statement to retrieve messages from DB")?;

        let query_map = statement
            .query_map(params![chat.id()], |row| {
                let id = ChatMessageId {
                    msg_id: row.get(0)?,
                };
                let sender = UserHandle {
                    user_id: row.get(1)?,
                };
                let timestamp: DateTime<Utc> = row.get(2)?;
                let message_str: String = row.get(3)?;
                let is_action: bool = row.get(4)?;
                let complete: bool = row.get_raw(5) == ValueRef::Null;

                let message = if is_action {
                    Message::Action(message_str)
                } else {
                    Message::Normal(message_str)
                };

                Ok(ChatLogEntry {
                    id,
                    sender,
                    message,
                    timestamp,
                    complete,
                })
            })
            .context("Failed to retrieve messages from DB")?;

        Ok(query_map
            .into_iter()
            .map(|item| item.map_err(Error::from))
            .collect::<Result<Vec<_>>>()
            .context("Failed to convert messages from DB")?)
    }

    pub fn add_unresolved_message(&mut self, message_id: &ChatMessageId) -> Result<()> {
        self.connection
            .execute(
                "INSERT OR REPLACE INTO pending_messages (message_id) VALUES (?1)",
                params![message_id.msg_id],
            )
            .context("Failed to insert receipt into DB")?;
        Ok(())
    }

    pub fn resolve_message(
        &mut self,
        _chat_handle: &ChatHandle,
        message_id: &ChatMessageId,
    ) -> Result<()> {
        self.connection
            .execute(
                "DELETE FROM pending_messages WHERE message_id = ?1",
                params![message_id.msg_id],
            )
            .context("Failed to remove receipt from DB")?;

        Ok(())
    }

    pub fn unresovled_messages(&mut self, chat_handle: &ChatHandle) -> Result<Vec<UnsentMessage>> {
        let mut statement = self
            .connection
            .prepare(
                "SELECT messages.id, text_messages.message, text_messages.action \
                FROM messages \
                JOIN pending_messages \
                ON pending_messages.message_id = messages.id \
                JOIN text_messages \
                ON messages.id = text_messages.message_id \
                WHERE messages.chat_id = ?1",
            )
            .context("Failed to prepare unresolved message query")?;

        let res = statement
            .query_map(params![chat_handle.chat_id], |row| {
                let id: i64 = row.get(0)?;
                let message_str = row.get(1)?;
                let action = row.get(2)?;

                let message = match action {
                    true => Message::Action(message_str),
                    false => Message::Normal(message_str),
                };

                Ok(UnsentMessage {
                    id: ChatMessageId { msg_id: id },
                    message,
                })
            })
            .context("Failed to query unresolved messages")?
            .into_iter()
            .map(|item| item.map_err(Error::from))
            .collect::<Result<Vec<_>>>();

        res
    }
}

fn initialize_db(connection: &mut Connection, self_pk: &PublicKey, self_name: &str) -> Result<()> {
    let transaction = connection.transaction()?;

    transaction
        .execute("PRAGMA foreign_keys = ON", NO_PARAMS)
        .context("Failed to enable foreign key support")?;

    // Create a chat id table that acts solely to link messages to
    // friends/groups
    transaction
        .execute(
            "CREATE TABLE IF NOT EXISTS chats (\
            id INTEGER PRIMARY KEY)",
            NO_PARAMS,
        )
        .context("Failed to create chats table")?;

    transaction
        .execute(
            "CREATE TABLE IF NOT EXISTS users (\
            id INTEGER PRIMARY KEY, \
            public_key BLOB NOT NULL UNIQUE,\
            name TEXT)",
            NO_PARAMS,
        )
        .context("Failed to create users table")?;

    // Friends is split from users since we know groups will be coming in later
    transaction
        .execute(
            "CREATE TABLE IF NOT EXISTS friends (\
            id INTEGER PRIMARY KEY, \
            user_id INTEGER NOT NULL, \
            chat_id INTEGER NOT NULL, \
            FOREIGN KEY (user_id) REFERENCES users(id), \
            FOREIGN KEY (chat_id) REFERENCES chat_id(id))",
            NO_PARAMS,
        )
        .context("Failed to create friends table")?;

    transaction
        .execute(
            "CREATE TABLE IF NOT EXISTS messages (\
            id INTEGER PRIMARY KEY, \
            chat_id INTEGER NOT NULL, \
            sender_id INTEGER NOT NULL, \
            timestamp TEXT NOT NULL, \
            FOREIGN KEY (chat_id) REFERENCES chats(id), \
            FOREIGN KEY (sender_id) REFERENCES users(id))",
            NO_PARAMS,
        )
        .context("Failed to create messages table")?;

    // Text messages are separate from messages since we know that file
    // transfers are incoming
    transaction
        .execute(
            "CREATE TABLE IF NOT EXISTS text_messages (\
            id INTEGER PRIMARY KEY, \
            message_id INTEGER NOT NULL, \
            message BLOB NOT NULL, \
            action BOOL NOT NULL, \
            FOREIGN KEY (message_id) REFERENCES messages(id))",
            NO_PARAMS,
        )
        .context("Failed to create text_messages table")?;

    // Receipt may be null to indicate an unsent pending message
    transaction
        .execute(
            "CREATE TABLE IF NOT EXISTS pending_messages (\
            id INTEGER PRIMARY KEY, \
            message_id INTEGER NOT NULL, \
            receipt_id INTEGER, \
            FOREIGN KEY (message_id) REFERENCES messages(id))",
            NO_PARAMS,
        )
        .context("Failed to create pending_messages table")?;

    transaction
        .execute(
            "CREATE TABLE IF NOT EXISTS pending_friends (\
            id INTEGER PRIMARY KEY, \
            user_id INTEGER NOT NULL, \
            FOREIGN KEY (user_id) REFERENCES users(id))",
            NO_PARAMS,
        )
        .context("Failed to create pending_friends table")?;

    let public_key = transaction
        .query_row(
            "SELECT public_key FROM users WHERE id = ?1",
            params![SELF_USER_ID],
            |row| {
                let pk: Vec<u8> = row.get(0)?;
                Ok(pk)
            },
        )
        .optional()
        .context("Failed to get self user public key")?;

    if let Some(public_key) = public_key {
        if self_pk.as_bytes() != public_key {
            return Err(anyhow!("DB already used by another user"));
        }
    }

    // NOTE: We insert our name into the DB, but we never actually read it back.
    // We might as well populate it correctly in case we want it in the future
    // though
    transaction
        .execute(
            "INSERT OR REPLACE INTO users (id, public_key, name) \
            VALUES (?1, ?2, ?3)",
            params![SELF_USER_ID, self_pk.as_bytes(), self_name],
        )
        .context("Failed to update self info")?;

    transaction
        .commit()
        .context("Failed to commit db initialization")?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_friend() -> Result<(), Error> {
        let selfpk = PublicKey::from_bytes(vec![0xff; PublicKey::SIZE])?;
        let mut storage = Storage::open_ram(&selfpk, "self")?;
        let pk1 = PublicKey::from_bytes(vec![1; PublicKey::SIZE])?;
        let pk2 = PublicKey::from_bytes(vec![2; PublicKey::SIZE])?;

        let returned_friend1 = storage.add_friend(pk1.clone(), "name1".to_string())?;
        let returned_friend2 = storage.add_friend(pk2.clone(), "name2".to_string())?;

        let friends = storage.friends()?;

        // Ensure added friends exist and are not marked as pending
        let retrieved_friend1 = friends
            .iter()
            .find(|friend| *friend.public_key() == pk1)
            .unwrap();
        assert_eq!(retrieved_friend1.name(), "name1");
        assert_ne!(*retrieved_friend1.status(), Status::Pending);

        let retrieved_friend2 = friends
            .iter()
            .find(|friend| *friend.public_key() == pk2)
            .unwrap();
        assert_eq!(retrieved_friend2.name(), "name2");
        assert_ne!(*retrieved_friend2.status(), Status::Pending);

        // Ensure added friends do not share the same chat handle
        assert_ne!(
            retrieved_friend1.chat_handle(),
            retrieved_friend2.chat_handle()
        );

        // Ensure that retrieved friends match the inserted friends
        assert_eq!(
            returned_friend1.public_key(),
            retrieved_friend1.public_key()
        );
        assert_eq!(returned_friend1.id(), retrieved_friend1.id());
        assert_eq!(
            returned_friend1.chat_handle(),
            retrieved_friend1.chat_handle()
        );
        assert_eq!(returned_friend1.name(), retrieved_friend1.name());

        assert_eq!(
            returned_friend2.public_key(),
            retrieved_friend2.public_key()
        );
        assert_eq!(returned_friend2.id(), retrieved_friend2.id());
        assert_eq!(
            returned_friend2.chat_handle(),
            retrieved_friend2.chat_handle()
        );
        assert_eq!(returned_friend2.name(), retrieved_friend2.name());
        Ok(())
    }

    #[test]
    fn duplicate_friend() -> Result<(), Error> {
        let selfpk = PublicKey::from_bytes(vec![0xff; PublicKey::SIZE])?;
        let mut storage = Storage::open_ram(&selfpk, "self")?;
        let pk1 = PublicKey::from_bytes(vec![1; PublicKey::SIZE])?;

        let friend_before = storage.add_friend(pk1.clone(), "name1".to_string())?;
        storage.add_friend(pk1.clone(), "name2".to_string())?;

        let friends = storage.friends()?;

        // Ensure that duplicated friend does not create a second friend
        assert_eq!(friends.len(), 1);

        // Ensure that duplicated friend has the same public key
        let friend_after = &friends[0];
        assert_eq!(*friend_before.public_key(), pk1);
        assert_eq!(*friend_after.public_key(), pk1);

        // Ensure that the second insert takes precedence
        assert_eq!(friend_after.name(), "name2");

        // Ensure that the duplicated friend does not create a new chat_handle/user
        assert_eq!(friend_before.chat_handle(), friend_after.chat_handle());
        assert_eq!(friend_before.id(), friend_after.id());

        Ok(())
    }

    #[test]
    fn friend_existing_user() -> Result<(), Error> {
        let selfpk = PublicKey::from_bytes(vec![0xff; PublicKey::SIZE])?;
        let mut storage = Storage::open_ram(&selfpk, "self")?;
        let pk1 = PublicKey::from_bytes(vec![1; PublicKey::SIZE])?;

        let user = storage.add_user(pk1.clone(), "name".to_string())?;
        let friend = storage.add_friend(pk1.clone(), "name2".to_string())?;

        // Second insertion should take precedence
        assert_eq!(friend.name(), "name2");
        // Friend id should match the previously inserted user (indicating a
        // second user was not inserted into the DB)
        assert_eq!(*friend.id(), user);

        Ok(())
    }

    #[test]
    fn numeric_name() -> Result<(), Error> {
        // Some bugs were seen where we made a mistake on the data type of the
        // sqlite column. This resulted in the value being inserted as an int
        // and failing to be read back out as a string
        let selfpk = PublicKey::from_bytes(vec![0xff; PublicKey::SIZE])?;
        let mut storage = Storage::open_ram(&selfpk, "self")?;
        let pk1 = PublicKey::from_bytes(vec![1; PublicKey::SIZE])?;
        storage.add_friend(pk1.clone(), "1234".to_string())?;

        let retrieved_friends = storage.friends()?;
        assert_eq!(retrieved_friends.len(), 1);
        assert_eq!(retrieved_friends[0].name(), "1234");

        Ok(())
    }

    #[test]
    fn friend_request() -> Result<(), Error> {
        let selfpk = PublicKey::from_bytes(vec![0xff; PublicKey::SIZE])?;
        let mut storage = Storage::open_ram(&selfpk, "self")?;
        let pk1 = PublicKey::from_bytes(vec![1; PublicKey::SIZE])?;

        let friend = storage.add_pending_friend(pk1)?;

        // Ensure returned friend does not get flagged as on online user
        assert_eq!(*friend.status(), Status::Pending);

        let retrieved_friends = storage.friends()?;

        // Ensure that the retrieved friend also is marked as pending
        assert_eq!(retrieved_friends.len(), 1);
        assert_eq!(*retrieved_friends[0].status(), Status::Pending);

        storage.resolve_pending_friend_request(friend.id())?;

        let retrieved_friends = storage.friends()?;

        // Ensure that after resolution the friend is no longer marked as pending
        assert_eq!(retrieved_friends.len(), 1);
        assert_ne!(*retrieved_friends[0].status(), Status::Pending);

        Ok(())
    }

    #[test]
    fn duplicate_user() -> Result<(), Error> {
        let selfpk = PublicKey::from_bytes(vec![0xff; PublicKey::SIZE])?;
        let mut storage = Storage::open_ram(&selfpk, "self")?;

        let pk1 = PublicKey::from_bytes(vec![1; PublicKey::SIZE])?;

        let user1 = storage.add_user(pk1.clone(), "test1".to_string())?;
        let user2 = storage.add_user(pk1.clone(), "test1".to_string())?;

        assert_eq!(user1, user2);

        Ok(())
    }

    #[test]
    fn self_user_handle() -> Result<(), Error> {
        let selfpk = PublicKey::from_bytes(vec![0xff; PublicKey::SIZE])?;
        let mut storage = Storage::open_ram(&selfpk, "self")?;

        // Ensure we can get our own user handle before any users have been
        // added to the db
        let user_handle = storage.self_user_handle();

        let pk1 = PublicKey::from_bytes(vec![1; PublicKey::SIZE])?;
        let pk2 = PublicKey::from_bytes(vec![2; PublicKey::SIZE])?;

        let user1 = storage.add_user(pk1, "test1".to_string())?;
        let user2 = storage.add_user(pk2, "test2".to_string())?;

        // We don't really have a constraint for what our user handle should be,
        // we just know it should be different than the user handles of our
        // peers
        assert_ne!(user_handle, user1);
        assert_ne!(user_handle, user2);

        Ok(())
    }

    #[test]
    fn message_history() -> Result<(), Error> {
        let selfpk = PublicKey::from_bytes(vec![0xff; PublicKey::SIZE])?;
        let mut storage = Storage::open_ram(&selfpk, "self")?;
        let self_user_handle = storage.self_user_handle();

        let pk1 = PublicKey::from_bytes(vec![1; PublicKey::SIZE])?;
        let pk2 = PublicKey::from_bytes(vec![2; PublicKey::SIZE])?;

        let friend1 = storage.add_friend(pk1, "test1".to_string())?;
        let friend2 = storage.add_friend(pk2, "test2".to_string())?;

        let start_time = Utc::now();
        // NOTE: we do not test for failure in the case where we add messages
        // with the wrong UID. The DB intentionally is designed to allow this
        // case. This makes it more flexible for when we eventually add group
        // messages
        storage.push_message(friend1.chat_handle(), self_user_handle, Message::Normal("msg1".into()))?;
        storage.push_message(friend2.chat_handle(), *friend2.id(), Message::Normal("msg2".into()))?;
        storage.push_message(friend2.chat_handle(), self_user_handle, Message::Action("msg3".into()))?;
        storage.push_message(friend1.chat_handle(), *friend1.id(), Message::Normal("msg4".into()))?;
        let end_time = Utc::now();

        // Ensure messages have the correct content after pulling from DB. We
        // will test message consistency with pending messages in another test
        let friend1_messages = storage.load_messages(friend1.chat_handle())?;
        assert_eq!(friend1_messages.len(), 2);
        assert_eq!(*friend1_messages[0].message(), Message::Normal("msg1".into()));
        assert_eq!(*friend1_messages[0].sender(), self_user_handle);
        assert_eq!(*friend1_messages[1].message(), Message::Normal("msg4".into()));
        assert_eq!(*friend1_messages[1].sender(), *friend1.id());

        let friend2_messages = storage.load_messages(friend2.chat_handle())?;
        assert_eq!(friend2_messages.len(), 2);
        assert_eq!(*friend2_messages[0].message(), Message::Normal("msg2".into()));
        assert_eq!(*friend2_messages[0].sender(), *friend2.id());
        assert_eq!(*friend2_messages[1].message(), Message::Action("msg3".into()));
        assert_eq!(*friend2_messages[1].sender(), self_user_handle);

        // Ensure that messages have reasonable timestamps relative to the times
        // we added them
        assert!(&start_time < friend1_messages[0].timestamp());
        assert!(friend1_messages[0].timestamp() < friend2_messages[0].timestamp());
        assert!(friend2_messages[0].timestamp() < friend2_messages[1].timestamp());
        assert!(friend2_messages[1].timestamp() < friend1_messages[1].timestamp());
        assert!(friend1_messages[1].timestamp() < &end_time);

        Ok(())
    }

    #[test]
    fn pending_messages() -> Result<(), Error> {
        let selfpk = PublicKey::from_bytes(vec![0xff; PublicKey::SIZE])?;
        let mut storage = Storage::open_ram(&selfpk, "self")?;

        let self_user_handle = storage.self_user_handle();
        let friend_pk = PublicKey::from_bytes(vec![1; PublicKey::SIZE])?;
        let friend = storage.add_friend(friend_pk, "test1".to_string())?;

        // Add a few of each message for normal case
        storage.push_message(friend.chat_handle(), self_user_handle, Message::Normal("msg1".into()))?;
        storage.push_message(friend.chat_handle(), self_user_handle, Message::Normal("msg2".into()))?;
        storage.push_message(friend.chat_handle(), self_user_handle, Message::Normal("msg3".into()))?;

        let unresolved_msg1 = storage.push_message(friend.chat_handle(), self_user_handle, Message::Normal("unresolved_msg1".into()))?;
        storage.add_unresolved_message(unresolved_msg1.id())?;
        let unresolved_msg2 = storage.push_message(friend.chat_handle(), self_user_handle, Message::Normal("unresolved_msg2".into()))?;
        storage.add_unresolved_message(unresolved_msg2.id())?;
        let unresolved_msg3 = storage.push_message(friend.chat_handle(), self_user_handle, Message::Normal("unresolved_msg3".into()))?;
        storage.add_unresolved_message(unresolved_msg3.id())?;

        // Ensure that unresolved messages in history are correct
        let unresolved_messages = storage.unresovled_messages(friend.chat_handle())?;
        assert_eq!(unresolved_messages.len(), 3);
        assert_eq!(unresolved_messages[0].id(), unresolved_msg1.id());
        assert_eq!(unresolved_messages[1].id(), unresolved_msg2.id());
        assert_eq!(unresolved_messages[2].id(), unresolved_msg3.id());
        assert_eq!(*unresolved_messages[0].message(), Message::Normal("unresolved_msg1".into()));
        assert_eq!(*unresolved_messages[1].message(), Message::Normal("unresolved_msg2".into()));
        assert_eq!(*unresolved_messages[2].message(), Message::Normal("unresolved_msg3".into()));

        // Ensure that loaded messages correctly mark completion state
        let loaded_messages = storage.load_messages(friend.chat_handle())?;
        assert_eq!(loaded_messages[0].complete(), true);
        assert_eq!(loaded_messages[1].complete(), true);
        assert_eq!(loaded_messages[2].complete(), true);
        assert_eq!(loaded_messages[3].complete(), false);
        assert_eq!(loaded_messages[4].complete(), false);
        assert_eq!(loaded_messages[5].complete(), false);

        // Resolve some messages (out of order)
        storage.resolve_message(friend.chat_handle(), unresolved_msg1.id())?;
        storage.resolve_message(friend.chat_handle(), unresolved_msg3.id())?;

        // Check that resolved messages correctly get marked as completed
        let unresolved_messages = storage.unresovled_messages(friend.chat_handle())?;
        assert_eq!(unresolved_messages.len(), 1);
        assert_eq!(unresolved_messages[0].id(), unresolved_msg2.id());

        let loaded_messages = storage.load_messages(friend.chat_handle())?;
        assert_eq!(loaded_messages[0].complete(), true);
        assert_eq!(loaded_messages[1].complete(), true);
        assert_eq!(loaded_messages[2].complete(), true);
        assert_eq!(loaded_messages[3].complete(), true);
        assert_eq!(loaded_messages[4].complete(), false);
        assert_eq!(loaded_messages[5].complete(), true);

        Ok(())

    }
}
