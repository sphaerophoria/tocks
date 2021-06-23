use thiserror::Error;

use toxcore_sys::*;

#[derive(Error, Debug)]
#[error("Not enough memory")]
pub struct ToxBuilderCreationError;

#[derive(Error, Debug)]
pub enum ToxCreationError {
    #[error("Unexpected null argument")]
    Null,
    #[error("Not enough memory")]
    Malloc,
    #[error("Unable to bind to port")]
    PortAlloc,
    #[error("Invalid proxy type")]
    BadProxyType,
    #[error("Invalid proxy host")]
    BadProxyHost,
    #[error("Invalid proxy port")]
    BadProxyPort,
    #[error("Proxy address could not be resolved")]
    ProxyNotFound,
    #[error("Encrypted save found in save data")]
    LoadEncrypted,
    #[error("Invalid data load format")]
    BadLoadFormat,
    #[error("Multiple instances created")]
    Multiple,
    #[error("Unknown creation error")]
    Unknown,
}

#[derive(Error, Debug)]
pub enum ToxBuildError {
    #[error("{0}")]
    ToxCreationError(#[from] ToxCreationError),
    #[error("Required callback not provided")]
    MissingCallbackError,
}

#[derive(Error, Debug)]
pub enum ToxAddFriendError {
    #[error("Invalid key")]
    InvalidKey,
    #[error("Unexpected null argument")]
    NullArgument,
    #[error("Add friend message too long")]
    MessageTooLong,
    #[error("Friend request message empty")]
    MessageEmpty,
    #[error("Cannot add self")]
    AddSelf,
    #[error("Request already sent")]
    AlreadySent,
    #[error("Bad checksum")]
    BadChecksum,
    #[error("Friend has new nospam")]
    NewNospam,
    #[error("Memory allocation failed")]
    Malloc,
    #[error("Unknown friend add error")]
    Unknown,
    #[error("{0}")]
    QueryError(#[from] ToxFriendQueryError),
}

impl From<u32> for ToxAddFriendError {
    fn from(err: u32) -> ToxAddFriendError {
        match err {
            TOX_ERR_FRIEND_ADD_NULL => return ToxAddFriendError::NullArgument,
            TOX_ERR_FRIEND_ADD_TOO_LONG => return ToxAddFriendError::MessageTooLong,
            TOX_ERR_FRIEND_ADD_NO_MESSAGE => return ToxAddFriendError::MessageEmpty,
            TOX_ERR_FRIEND_ADD_OWN_KEY => return ToxAddFriendError::AddSelf,
            TOX_ERR_FRIEND_ADD_ALREADY_SENT => return ToxAddFriendError::AlreadySent,
            TOX_ERR_FRIEND_ADD_BAD_CHECKSUM => return ToxAddFriendError::BadChecksum,
            TOX_ERR_FRIEND_ADD_SET_NEW_NOSPAM => return ToxAddFriendError::NewNospam,
            TOX_ERR_FRIEND_ADD_MALLOC => return ToxAddFriendError::Malloc,
            _ => return ToxAddFriendError::Unknown,
        }
    }
}

#[derive(Error, Debug)]
pub enum ToxFriendRemoveError {
    #[error("Friend not found")]
    NotFound,
    #[error("Unknown friend remove error")]
    Unknown,
}

impl From<u32> for ToxFriendRemoveError {
    fn from(err: u32) -> ToxFriendRemoveError {
        match err {
            TOX_ERR_FRIEND_DELETE_FRIEND_NOT_FOUND => return ToxFriendRemoveError::NotFound,
            _ => return ToxFriendRemoveError::Unknown,
        }
    }
}

#[derive(Error, Debug)]
pub enum ToxFriendQueryError {
    #[error("Invalid argument")]
    InvalidArgument,
    #[error("Friend not found")]
    NotFound,
    #[error("Unknown friend query error")]
    Unknown,
}

impl From<u32> for ToxFriendQueryError {
    fn from(err: u32) -> ToxFriendQueryError {
        match err {
            TOX_ERR_FRIEND_QUERY_NULL => ToxFriendQueryError::InvalidArgument,
            TOX_ERR_FRIEND_QUERY_FRIEND_NOT_FOUND => ToxFriendQueryError::NotFound,
            _ => ToxFriendQueryError::Unknown,
        }
    }
}

#[derive(Error, Debug)]
pub enum ToxSendMessageError {
    #[error("Tox instance no longer valid")]
    NoTox(#[from] ToxDestructedError),
    #[error("Invalid argument")]
    InvalidArgument,
    #[error("Invalid friend id")]
    InvalidFriendId,
    #[error("Not connected")]
    NotConnected,
    #[error("Internal malloc error")]
    InternalError,
    #[error("Message too long")]
    MessageTooLong,
    #[error("Message empty")]
    MessageEmpty,
    #[error("Unknown")]
    Unknown,
}

impl From<u32> for ToxSendMessageError {
    fn from(err: u32) -> ToxSendMessageError {
        match err {
            TOX_ERR_FRIEND_SEND_MESSAGE_NULL => ToxSendMessageError::InvalidArgument,
            TOX_ERR_FRIEND_SEND_MESSAGE_FRIEND_NOT_FOUND => ToxSendMessageError::InvalidFriendId,
            TOX_ERR_FRIEND_SEND_MESSAGE_FRIEND_NOT_CONNECTED => ToxSendMessageError::NotConnected,
            TOX_ERR_FRIEND_SEND_MESSAGE_SENDQ => ToxSendMessageError::InternalError,
            TOX_ERR_FRIEND_SEND_MESSAGE_TOO_LONG => ToxSendMessageError::MessageTooLong,
            TOX_ERR_FRIEND_SEND_MESSAGE_EMPTY => ToxSendMessageError::MessageEmpty,
            _ => ToxSendMessageError::Unknown,
        }
    }
}

#[derive(Error, Debug)]
pub enum KeyDecodeError {
    #[error("Invalid hex {0}")]
    Hex(#[from] hex::FromHexError),
    #[error("Invalid key length (actual: {actual}, expected: {expected})")]
    InvalidKeyLength { actual: usize, expected: usize },
}

#[derive(Error, Debug)]
#[error("Tox instance no longer valid")]
pub struct ToxDestructedError;

#[derive(Error, Debug)]
#[error("Failed to derive key from provided input")]
pub struct KeyDerivationError;

#[derive(Error, Debug)]
#[error("Encryption failed")]
pub struct EncryptionError;

#[derive(Error, Debug)]
#[error("Decryption failed")]
pub struct DecryptionError;

#[derive(Error, Debug)]
#[error("Info too long")]
pub struct SetInfoError;
