use crate::{
    contact::Friend,
    error::{Error, Result},
};

use toxcore::{Message, PublicKey, Receipt};

use chrono::{DateTime, Utc};
use rusqlite::{Connection, NO_PARAMS, OptionalExtension, Transaction, params};

use std::path::Path;


// Wrapper around sqlite message table id
pub struct ChatMessageId
{
    msg_id: i64,
}

// NOTE: This is written to the DB, so if the meanings of these values are
// changed you may have data consistency issues
pub struct ChatLogEntry
{
    id: ChatMessageId,
    sender: UserHandle,
    message: Message,
    timestamp: DateTime<Utc>,
}

impl ChatLogEntry
{
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
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ChatHandle
{
    chat_id: i64,
}

impl ChatHandle
{
    pub fn id(&self) -> i64 {
        self.chat_id
    }
}

impl From<i64> for ChatHandle
{
    fn from(id: i64) -> Self {
        Self { chat_id: id }
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct UserHandle
{
    user_id: i64,
}

impl UserHandle
{
    pub fn id(&self) -> i64
    {
        self.user_id
    }
}

pub(crate) struct Storage
{
    ram_db: bool,
    connection: Connection,
}

impl Storage
{
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Storage>
    {
        let mut connection = Connection::open(&path)?;
        initialize_db(&mut connection)?;
        Ok(Storage {
            ram_db: false,
            connection,
        })
    }

    pub fn open_ram() -> Result<Storage>
    {
        let mut connection = Connection::open_in_memory()?;

        initialize_db(&mut connection)?;
        Ok(Storage {
            ram_db: true,
            connection,
        })
    }

    pub fn friends(&self) -> Result<Vec<Friend>>
    {
        let mut statement = self.connection.prepare(
            "SELECT chat_id, user_id, users.public_key, users.name \
                FROM friends LEFT JOIN users ON user_id = users.id")?;

        let query_results = statement.query_map(
            NO_PARAMS,
            |row| {
                let chat_handle = ChatHandle { chat_id: row.get(0)? };
                let user_handle = UserHandle { user_id: row.get(1)? };
                let public_key_bytes: Vec<u8> = row.get(2)?;
                let name: String = row.get(3)?;

                Ok((chat_handle, user_handle, public_key_bytes, name))
            })?;


        query_results
            .into_iter()
            .filter(std::result::Result::is_ok)
            .map(|item| item.unwrap())
            .map(|(chat_handle, user_handle, public_key_bytes, name)| {
                Ok(Friend::new(user_handle, chat_handle, PublicKey::from_bytes(public_key_bytes)?, name))
            })
            .collect::<Result<Vec<Friend>>>()
    }

    pub fn add_friend(&mut self, public_key: PublicKey, name: String) -> Result<Friend>
    {
        let transaction = self.connection.transaction()?;

        let user_id = Self::add_user_transaction(&transaction, &public_key, &name)?;

        transaction.execute("INSERT INTO chats DEFAULT VALUES", NO_PARAMS)?;
        let chat_id = transaction.last_insert_rowid();

        transaction.execute("INSERT INTO friends (user_id, chat_id) VALUES (?1, ?2)", params![user_id.id(), chat_id])?;
        let chat_id = transaction.last_insert_rowid();

        transaction.commit()?;


        Ok(Friend::new(user_id, ChatHandle { chat_id }, public_key, name))
    }

    pub fn add_user(&mut self, public_key: PublicKey, name: String) -> Result<UserHandle>
    {
        let transaction = self.connection.transaction()?;
        let handle = Self::add_user_transaction(&transaction, &public_key, &name)?;
        transaction.commit()?;
        Ok(handle)
    }

    fn add_user_transaction(transaction: &Transaction, public_key: &PublicKey, name: &String) -> Result<UserHandle>
    {
        // Check if user is already in users table
        let user_id = transaction.query_row("SELECT id FROM users where public_key = ?1", params![public_key.as_bytes()], |row| {
            let id: i64 = row.get(0)?;
            Ok(id)
        }).optional()?;

        let user_id = match user_id {
            Some(id) => {
                transaction.execute("UPDATE users SET name = ?2 WHERE id = ?1", params![id, name])?;
                id
            }
            None => {
                transaction.execute("INSERT INTO users (public_key, name) VALUES (?1, ?2)", params![public_key.as_bytes(), name])?;
                transaction.last_insert_rowid()
            }
        };

        Ok(UserHandle { user_id })
    }

    pub fn user_handle(&self, public_key: &PublicKey) -> Result<UserHandle> {
        let id = self.connection.query_row("SELECT id FROM users WHERE public_key = ?1", params![public_key.as_bytes()], |row| {
            let id: i64 = row.get(0)?;
            Ok(id)
        })?;

        Ok(UserHandle { user_id: id })
    }

    pub fn push_message(&mut self, chat: &ChatHandle, sender: UserHandle, message: Message) -> Result<ChatLogEntry>
    {
        let timestamp = Utc::now();

        let (message_str, is_action) = match &message {
            Message::Action(s) => (s, true),
            Message::Normal(s) => (s, false),
        };

        let transaction = self.connection.transaction()?;

        transaction.execute("INSERT INTO messages (chat_id, sender_id, timestamp) \
            VALUES (?1, ?2, ?3)", params![chat.chat_id, sender.user_id, timestamp])?;

        let id = ChatMessageId{ msg_id: transaction.last_insert_rowid() };

        transaction.execute("INSERT INTO text_messages (message_id, message, action) \
            VALUES (?1, ?2, ?3)", params![id.msg_id, message_str, is_action])?;

        transaction.commit()?;

        Ok(ChatLogEntry {
            id,
            sender,
            message,
            timestamp
        })
    }

    pub fn load_messages(&mut self, chat: &ChatHandle) -> Result<Vec<ChatLogEntry>> {

        let mut statement = self.connection.prepare("SELECT messages.id, sender_id, timestamp, message, action FROM messages LEFT JOIN text_messages ON messages.id = text_messages.message_id WHERE chat_id = ?1")?;

        let query_map = statement.query_map(params![chat.id()], |row| {
            let id = ChatMessageId { msg_id: row.get(0)? };
            let sender = UserHandle { user_id: row.get(1)? };
            let timestamp: DateTime<Utc> = row.get(2)?;
            let message_str: String = row.get(3)?;
            let is_action: bool = row.get(4)?;

            let message = if is_action {
                Message::Action(message_str)
            } else {
                Message::Normal(message_str)
            };

            Ok(ChatLogEntry {
                id,
                sender,
                message,
                timestamp
            })
        })?;

        Ok(query_map.into_iter().map(|item| item.map_err(Error::from)).collect::<Result<Vec<_>>>()?)
    }

    pub fn add_unresolved_receipt(&mut self, message_id: &ChatMessageId, receipt: &Receipt) -> Result<()> {
        self.connection.execute("INSERT OR REPLACE INTO pending_messages (message_id, receipt_id) VALUES (?1, ?2)", params![message_id.msg_id, receipt.id()])?;
        Ok(())
    }

    pub fn resolve_receipt(&mut self, receipt: &Receipt) -> Result<ChatMessageId> {
        let msg_id = self.connection.query_row("SELECT message_id FROM pending_messages WHERE receipt_id = ?1", params![receipt.id()], |row| row.get(0))?;
        self.connection.execute("DELETE FROM pending_messages WHERE receipt_id = ?1", params![receipt.id()])?;

        Ok(ChatMessageId {
            msg_id
        })
    }

}

fn initialize_db(connection: &mut Connection) -> Result<()> {
    let transaction = connection.transaction()?;

    transaction.execute( "PRAGMA foreign_keys = ON", NO_PARAMS)?;

    // Create a chat id table that acts solely to link messages to
    // friends/groups
    transaction.execute(
        "CREATE TABLE IF NOT EXISTS chats (\
            id INTEGER PRIMARY KEY)", NO_PARAMS)?;

    transaction.execute(
        "CREATE TABLE IF NOT EXISTS users (\
            id INTEGER PRIMARY KEY, \
            public_key BLOB NOT NULL UNIQUE,\
            name STRING)", NO_PARAMS)?;

    // Friends is split from users since we know groups will be coming in later
    transaction.execute(
        "CREATE TABLE IF NOT EXISTS friends (\
            id INTEGER PRIMARY KEY, \
            user_id INTEGER NOT NULL, \
            chat_id INTEGER NOT NULL, \
            FOREIGN KEY (user_id) REFERENCES users(id), \
            FOREIGN KEY (chat_id) REFERENCES chat_id(id))", NO_PARAMS)?;

    transaction.execute(
        "CREATE TABLE IF NOT EXISTS messages (\
            id INTEGER PRIMARY KEY, \
            chat_id INTEGER NOT NULL, \
            sender_id INTEGER NOT NULL, \
            timestamp STRING NOT NULL, \
            FOREIGN KEY (chat_id) REFERENCES chats(id), \
            FOREIGN KEY (sender_id) REFERENCES users(id))", NO_PARAMS)?;

    // Text messages are separate from messages since we know that file
    // transfers are incoming
    transaction.execute(
        "CREATE TABLE IF NOT EXISTS text_messages (\
            id INTEGER PRIMARY KEY, \
            message_id INTEGER NOT NULL, \
            message BLOB NOT NULL, \
            action BOOL NOT NULL, \
            FOREIGN KEY (message_id) REFERENCES messages(id))", NO_PARAMS)?;

    // Receipt may be null to indicate an unsent pending message
    transaction.execute(
        "CREATE TABLE IF NOT EXISTS pending_messages (\
            id INTEGER PRIMARY KEY, \
            message_id INTEGER NOT NULL, \
            receipt_id INTEGER, \
            FOREIGN KEY (message_id) REFERENCES messages(id))", NO_PARAMS)?;

    transaction.commit()?;

    clear_receipt_ids(connection)?;

    Ok(())
}

pub fn clear_receipt_ids(connection: &mut Connection) -> Result<()> {
    connection.execute("UPDATE pending_messages SET receipt_id = NULL", NO_PARAMS)?;

    Ok(())
}
