// Use unix sockets on platforms that support them, but fall back to less
// desirable tcp sockets on others
#[cfg(target_family = "unix")]
mod unix;
#[cfg(not(target_family = "unix"))]
mod tcp;

#[cfg(target_family = "unix")]
use unix::*;
#[cfg(not(target_family = "unix"))]
use tcp::*;

use crate::{TocksEvent, TocksUiEvent};

use anyhow::{Context, Result};
use log::{error, info};
use futures::{
    channel::mpsc::{UnboundedSender, UnboundedReceiver},
    FutureExt,
    Stream,
    StreamExt,
};

use tokio::io::{
    AsyncWriteExt,
    AsyncBufReadExt,
    BufReader,
};

use std::{
    task::Poll,
};

pub struct EventServer {
    tocks_event_rx: UnboundedReceiver<TocksEvent>,
    tocks_event_tx: UnboundedSender<TocksEvent>,
    ui_event_tx: UnboundedSender<TocksUiEvent>,
    event_client_listener: Listener,
    clients: Vec<EventStream>,
}

impl EventServer {
    pub fn new(
        tocks_event_rx: UnboundedReceiver<TocksEvent>,
        tocks_event_tx: UnboundedSender<TocksEvent>,
        ui_event_tx: UnboundedSender<TocksUiEvent>) -> Result<EventServer> {

            let socket_path = get_socket_addr();
            let event_client_listener = create_event_client_listener(socket_path)
                .context("Failed to create event client listener")?;

            Ok(EventServer {
                tocks_event_rx,
                tocks_event_tx,
                ui_event_tx,
                event_client_listener,
                clients: Default::default(),
            })
    }

    pub async fn run(&mut self) -> Result<()> {
        loop {
            futures::select! {
                client = wait_for_client(&mut self.event_client_listener).fuse() => {
                    match client {
                        Ok(client) => self.clients.push(client),
                        Err(e) => error!("Failed to handle new event client: {}", e),
                    }
                }
                tocks_event = self.tocks_event_rx.next() => {
                    // FIXME: Better error handling
                    if let Err(e) = self.handle_tocks_event(tocks_event).await {
                        error!("{}", e);
                    }
                }
                ui_event = wait_for_ui_event(&mut self.clients).fuse() => {
                    if let Err(e) = self.handle_ui_event(ui_event) {
                        error!("Failed to handle incoming event: {}", e);
                    }
                }
            }
        }
    }

    async fn handle_tocks_event(&mut self, event: Option<TocksEvent>) -> Result<()> {
        if event.is_none() {
            anyhow::bail!("No more tocks events");
        }

        let event = event.unwrap();

        let mut serialized = serde_json::to_vec(&event)
            .context("Failed to serialize event")?;
        serialized.push(b'\n');

        self.tocks_event_tx.unbounded_send(event)
            .context("Failed to propogate event")?;

        let mut clients_to_remove = vec![];
        for (idx, client) in self.clients.iter_mut().enumerate() {
            if client.write_all(&serialized).await.is_err() {
                clients_to_remove.push(idx);
            }
        }

        for client in clients_to_remove.into_iter().rev() {
            info!("Removing client {}", client);
            self.clients.remove(client);
        }

        Ok(())
    }

    fn handle_ui_event(&mut self, ui_event: Result<Option<TocksUiEvent>>) -> Result<()> {
        let ui_event = ui_event?;
        if let Some(ui_event) = ui_event {
            self.ui_event_tx.unbounded_send(ui_event)?;
        }
        Ok(())
    }
}

pub struct EventClient {
    socket_stream: BufReader<EventStream>,
}

impl EventClient {
    pub async fn connect() -> Result<EventClient> {
        let path = get_socket_addr();
        let connection = EventStream::connect(path)
            .await
            .context("Failed to create event client")?;

        let buffered_reader = BufReader::new(connection);

        Ok(EventClient {
            socket_stream: buffered_reader,
        })
    }

    pub async fn send(&mut self, event: TocksUiEvent) -> Result<()> {
        let stream = self.socket_stream.get_mut();
        let mut serialized = serde_json::to_vec(&event)?;
        serialized.push(b'\n');

        stream.write_all(&serialized)
            .await
            .context("Failed to send tocks event")?;

        Ok(())
    }
}

impl Stream for EventClient {
    type Item = Result<TocksEvent>;

    fn poll_next(
        mut self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>
    ) -> core::task::Poll<Option<Self::Item>> {
        let mut v = Vec::new();
        let res = {
            let mut stream = self.socket_stream.read_until(b'\n', &mut v).boxed();
            let pin = stream.as_mut();
            pin.poll(cx)
        };
        match res {
            Poll::Ready(Ok(size)) => {
                if size == 0 {
                    return Poll::Ready(None)
                }
                let res = serde_json::from_slice(&v)
                    .map_err(anyhow::Error::from);
                Poll::Ready(Some(res))
            },
            Poll::Ready(Err(e)) => {
                error!("Failed to read from event server: {}", e);
                Poll::Ready(None)
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

async fn wait_for_client(client_listener: &mut Listener) -> Result<EventStream> {
    Ok(client_listener.accept().await?.0)
}

async fn wait_for_ui_event_from_client(client: &mut EventStream) -> Result<Option<TocksUiEvent>> {
    let mut buf = Vec::new();
    let res = BufReader::new(client.split().0).read_until(b'\n', &mut buf).await?;
    if res == 0 {
        return Ok(None)
    }

    let event = serde_json::from_slice(&buf)
        .context("Failed to parse ui event")?;

    Ok(Some(event))
}


async fn wait_for_ui_event(clients: &mut Vec<EventStream>) -> Result<Option<TocksUiEvent>> {
    if clients.is_empty() {
        // If there are no clients we block forever to avoid waking up our event
        // loop
        futures::future::pending::<()>().await;
    }

   let next_event_futures = clients.iter_mut().map(|client| wait_for_ui_event_from_client(client).boxed());
    futures::future::select_all(next_event_futures).await.0
}


#[cfg(test)]
mod tests
{
    use super::*;
    use futures::channel::mpsc;
    use lazy_static::lazy_static;
    use std::sync::{Mutex, MutexGuard};
    use futures::SinkExt;

    lazy_static! {
        static ref SINGLE_INSTANCE: Mutex<()> = Mutex::new(());
    }

    struct Fixture {
        client: EventClient,
        server: EventServer,
        ui_channel_rx: UnboundedReceiver<TocksUiEvent>,
        tocks_event_tx: UnboundedSender<TocksEvent>,
        event_server_rx: UnboundedReceiver<TocksEvent>,
        _single_instance_guard: MutexGuard<'static, ()>,
    }

    impl Fixture {
        async fn new() -> Result<Fixture> {
            let guard = SINGLE_INSTANCE.lock().unwrap();
            let tocks_event_channel = mpsc::unbounded();
            let event_server_channel = mpsc::unbounded();
            let ui_event_channel = mpsc::unbounded();

            let mut server = EventServer::new(
                tocks_event_channel.1,
                event_server_channel.0,
                ui_event_channel.0)?;

            // Run the server until the connection handshake completes
            let mut fixture  = futures::select! {
                client = EventClient::connect().fuse() => {
                    Fixture {
                        client: client.unwrap(),
                        server,
                        ui_channel_rx: ui_event_channel.1,
                        tocks_event_tx: tocks_event_channel.0,
                        event_server_rx: event_server_channel.1,
                        _single_instance_guard: guard,
                    }
                }
                _ = server.run().fuse() => {
                    panic!("Server exited early");
                }
            };

            // Run the server for a little. There seems to be a race where the
            // server may not have accepted the connection by the time the client
            // returns
            futures::select! {
                _ = fixture.server.run().fuse() => (),
                _ = tokio::time::sleep(std::time::Duration::from_millis(20)).fuse() => (),
            }

            Ok(fixture)
        }
    }

    struct Fixture2Client {
        client1: EventClient,
        client2: EventClient,
        server: EventServer,
        ui_channel_rx: UnboundedReceiver<TocksUiEvent>,
        tocks_event_tx: UnboundedSender<TocksEvent>,
        _event_server_rx: UnboundedReceiver<TocksEvent>,
        _single_instance_guard: MutexGuard<'static, ()>,
    }

    impl Fixture2Client {
        async fn new() -> Result<Fixture2Client> {
            let mut fixture1 = Fixture::new().await?;

            let client = futures::select! {
                _ = fixture1.server.run().fuse() => panic!("Unexpected server exit"),
                client = EventClient::connect().fuse() => client,
            }?;

            // Run the server for a little. There seems to be a race where the
            // server may not have accepted the connection by the time the client
            // returns
            futures::select! {
                _ = fixture1.server.run().fuse() => (),
                _ = tokio::time::sleep(std::time::Duration::from_millis(20)).fuse() => (),
            }

            Ok(Fixture2Client {
                client1: fixture1.client,
                client2: client,
                server: fixture1.server,
                ui_channel_rx: fixture1.ui_channel_rx,
                tocks_event_tx: fixture1.tocks_event_tx,
                _event_server_rx: fixture1.event_server_rx,
                _single_instance_guard: fixture1._single_instance_guard
            })
        }
    }


    #[tokio::test]
    async fn test_tocks_event_propagation() -> Result<()> {
        // Ensure that when a tocks event is sent it's correctly propagated to
        // both the UI and to the event client

        let mut fixture = Fixture::new().await?;

        fixture.tocks_event_tx.send(TocksEvent::Error("Test".to_owned())).await?;

        // Run the server until we receive our event in the client
        let received = futures::select! {
            _ = fixture.server.run().fuse() => {
                panic!("Server exited early");
            }
            received = fixture.client.next().fuse() => {
                received
            }
        };

        let propagated = fixture.event_server_rx.next().await;

        let check_expected_event = |event| {
            match event {
                Some(TocksEvent::Error(e)) => {
                    assert_eq!(e, "Test");
                }
                _ => assert!(false),
            };
        };

        check_expected_event(propagated);
        check_expected_event(received.transpose()?);

        Ok(())
    }

    #[tokio::test]
    async fn test_tocks_ui_event_propagation() -> Result<()> {
        // Ensure that when the client sends a UI event it gets propagated to
        // the main tocks instance

        let mut fixture = Fixture::new().await?;
        fixture.client.send(TocksUiEvent::Close).await?;

        // Run the server until we receive and propagate the event
        let event = futures::select! {
            received = fixture.ui_channel_rx.next() => {
                received
            }
            _ = fixture.server.run().fuse() => {
                panic!("Server exited unexpectedly");
            }
        };

        match event {
            Some(TocksUiEvent::Close) => {}
            _ => panic!("Unexpected event"),
        }

        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_client_recv() -> Result<()> {
        let mut fixture = Fixture2Client::new().await?;

        fixture.tocks_event_tx.unbounded_send(TocksEvent::Error("Error".to_string()))?;

        let clients_next = futures::future::join(fixture.client1.next(), fixture.client2.next());

        let (result1, result2) = futures::select! {
            res = clients_next.fuse() => res,
            _ = fixture.server.run().fuse() => panic!("Server exited early"),
        };

        let check_event =  |event| {
            match event {
                Some(Ok(TocksEvent::Error(e))) => {
                    assert_eq!(e, "Error")
                }
                _ => panic!("Unexpected event"),
            }
        };

        check_event(result1);
        check_event(result2);

        Ok(())
    }

    #[tokio::test]
    async fn test_multiple_client_send() -> Result<()> {
        let mut fixture = Fixture2Client::new().await?;

        fixture.client1.send(TocksUiEvent::Close).await?;
        fixture.client2.send(TocksUiEvent::CreateAccount("Test".into(), "password".into())).await?;

        let server = &mut fixture.server;
        let ui_channel_rx = &mut fixture.ui_channel_rx;

        let next_vals = async {
            let first = ui_channel_rx.next().await;
            let second = ui_channel_rx.next().await;
            (first, second)
        };

        let (first, second) = futures::select! {
            res = next_vals.fuse() => res,
            _ = server.run().fuse() => panic!("Unexpected server end"),
        };

        match first {
            Some(TocksUiEvent::Close) => {},
            _ => panic!("Unexpected ui event"),
        }

        match second {
            Some(TocksUiEvent::CreateAccount(user, pass)) => {
                assert_eq!(user, "Test");
                assert_eq!(pass, "password");
            },
            _ => panic!("Unexpected second ui event"),
        }

        Ok(())
    }
}
