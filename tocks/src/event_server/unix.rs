use anyhow::Result;
use std::{
    env,
    path::PathBuf
};

pub type Listener = tokio::net::UnixListener;
pub type EventStream = tokio::net::UnixStream;
pub type EventServerAddr = PathBuf;

pub fn get_socket_addr() -> EventServerAddr {
    let mut path = env::temp_dir();
    path.push("tocks.sock");
    path
}

pub fn create_event_client_listener(socket_path: EventServerAddr) -> Result<Listener> {
    // Best effort removal, if we fail for a good reason the bind call will fail
    // too.
    //
    // FIXME: If a second tocks instance is opened we nuke the path of the first
    // one. We should add a tocks instance lock instead of just an account lock
    let _ = std::fs::remove_file(&socket_path);
    Ok(Listener::bind(socket_path)?)
}
