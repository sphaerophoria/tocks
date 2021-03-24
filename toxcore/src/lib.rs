#![allow(clippy::needless_return)]
#![allow(clippy::mutex_atomic)]

//! Rust bindings for Tox, a peer to peer, end to end encrypted instant messenger.

pub mod error;

mod builder;
mod encryption;
mod friend;
mod sys;
mod tox;

pub use crate::{builder::ToxBuilder, friend::Friend, tox::Tox, encryption::PassKey};
use error::*;

use toxcore_sys::{TOX_PUBLIC_KEY_SIZE, TOX_SECRET_KEY_SIZE};

use hex::FromHex;

use std::fmt;

pub enum SaveData {
    ToxSave(Vec<u8>),
    SecretKey(Vec<u8>),
    None,
}
pub enum ProxyType {
    None,
    Http,
    Socks5,
}

macro_rules! impl_key_type {
    ($name:ident, $underlying_type:ty, $expected_size:expr) => {
        #[derive(Clone, Debug, PartialEq, Eq, Hash)]
        pub struct $name {
            key: $underlying_type,
        }

        impl $name {
            pub fn as_bytes(&self) -> &[u8] {
                &self.key
            }

            pub fn from_bytes(key: Vec<u8>) -> Result<$name, KeyDecodeError> {
                let expected_length = $expected_size as usize;

                if key.len() != expected_length {
                    return Err(KeyDecodeError::InvalidKeyLength {
                        actual: key.len(),
                        expected: expected_length,
                    });
                }

                Ok($name { key })
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                self.key.iter().try_for_each(|b| write!(f, "{:02x}", b))
            }
        }

        impl std::str::FromStr for $name {
            type Err = KeyDecodeError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                let ret: $underlying_type = FromHex::from_hex(s)?;

                let expected_length = $expected_size as usize;

                if ret.len() != expected_length {
                    return Err(KeyDecodeError::InvalidKeyLength {
                        actual: ret.len(),
                        expected: expected_length,
                    });
                }

                Ok($name { key: ret })
            }
        }
    };
}

// FIXME: sizes should be retrieved through API class
impl_key_type!(PublicKey, Vec<u8>, TOX_PUBLIC_KEY_SIZE);
impl_key_type!(SecretKey, Vec<u8>, TOX_SECRET_KEY_SIZE);
impl_key_type!(ToxId, Vec<u8>, TOX_PUBLIC_KEY_SIZE + 4 + 2);

/// Receipt for sent message
#[derive(Hash, PartialEq, Eq)]
pub struct Receipt {
    id: u32,
    friend: Friend,
}

impl Receipt {
    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn friend(&self) -> &Friend {
        &self.friend
    }
}

/// Incoming friend request
#[derive(Clone, Debug)]
pub struct FriendRequest {
    /// Public key of the user who issuer
    pub public_key: PublicKey,
    /// Message sent along with the request
    pub message: String,
}

/// Internal helper type to share data between the [`Tox`] instance and the
/// [`Friend`] handles it creates. This is necessary since the handles do not
/// have functions to respond to updates. This is designed to allow for
/// name change notifications, etc. to be processed in [`Tox::run`]
#[derive(Debug)]
pub(crate) struct FriendData {
    pub(crate) public_key: PublicKey,
    pub(crate) name: String,
    pub(crate) status: Status,
}

#[derive(Debug, Clone)]
pub enum Message {
    Normal(String),
    Action(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Online,
    Away,
    Busy,
    Offline,
}

pub enum Event {
    MessageReceived(Friend, Message),
    FriendRequest(FriendRequest),
    ReadReceipt(Receipt),
    StatusUpdated(Friend),
}
