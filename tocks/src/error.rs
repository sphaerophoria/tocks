use thiserror::Error;

#[derive(Error, Debug)]
pub enum ExitError {
    #[error("Exiting expectedly")]
    Graceful,
    #[error("Unexpected exit")]
    Ungraceful,
}
