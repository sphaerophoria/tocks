use thiserror::Error;

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
    #[error("Unknown creation error")]
    Unknown,
}

#[derive(Error, Debug)]
pub enum ToxFriendError {
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
    #[error("Failure to retrieve public key")]
    PublicKey,
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
