use anyhow::Result;

use std::net::SocketAddr;

pub type Listener = tokio::net::TcpListener;
pub type EventStream = tokio::net::TcpStream;
pub type EventServerAddr = SocketAddr;

pub fn get_socket_addr() -> EventServerAddr {
    "127.0.0.1:9304".parse().unwrap()
}

pub fn create_event_client_listener(socket_path: EventServerAddr) -> Result<Listener> {
    Ok(futures::executor::block_on(Listener::bind(socket_path))?)
}
