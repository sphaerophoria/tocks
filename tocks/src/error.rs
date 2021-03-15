use thiserror::Error;
use toxcore::error::*;

use std::sync::PoisonError;

#[derive(Error, Debug)]
#[error("{0}")]
pub enum Error {
    ToxBuildError(#[from] ToxBuilderCreationError),
    ToxCreationError(#[from] ToxCreationError),
    ToxAddFriendError(#[from] ToxAddFriendError),
    ToxFriendSendMessageError(#[from] ToxSendMessageError),
    // Early convert the poison error to avoid the lifetime issues with holding
    // the internal guard
    PoisonError(String),
    #[error("{0} is not implemented")]
    Unimplemented(String),
    IoError(#[from] std::io::Error),
    Misc(#[from] Box<dyn std::error::Error>),
    #[error("Invalid argument")]
    InvalidArgument,
}

impl<T> From<PoisonError<T>> for Error {
    fn from(err: PoisonError<T>) -> Self {
        Error::PoisonError(err.to_string())
    }
}

pub type Result<T> = core::result::Result<T, Error>;
