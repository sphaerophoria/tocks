#![allow(clippy::needless_return)]
#![allow(clippy::mutex_atomic)]

//! Rust bindings for Tox, a peer to peer, end to end encrypted instant messenger.

pub mod error;

mod builder;
mod friend;
mod sys;
mod tox;

pub use crate::{builder::ToxBuilder, friend::Friend, tox::Tox};
use error::*;

use toxcore_sys::{TOX_PUBLIC_KEY_SIZE, TOX_SECRET_KEY_SIZE};

use hex::FromHex;

use std::fmt;

pub enum SaveData<'a> {
    ToxSave(&'a [u8]),
    SecretKey(&'a [u8]),
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
pub struct Receipt {
    #[allow(dead_code)]
    id: u32,
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
struct FriendData {
    public_key: PublicKey,
    name: String,
}

#[derive(Debug, Clone)]
pub enum Message {
    Normal(String),
    Action(String),
}
