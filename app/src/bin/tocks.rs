use futures::{channel::mpsc, prelude::*};
use log::error;
use tocks::{EventServer, Tocks};
use ui::QmlUi;

#[tokio::main]
async fn main() {
    pretty_env_logger::init();

    let tocks_event_channel = mpsc::unbounded();
    let ui_event_channel = mpsc::unbounded();
    let event_server_channel = mpsc::unbounded();

    let _ui = QmlUi::new(ui_event_channel.0.clone(), event_server_channel.1);

    let mut event_server = EventServer::new(
        tocks_event_channel.1,
        event_server_channel.0,
        ui_event_channel.0,
    )
    .expect("Failed to start event server");

    let mut tocks = Tocks::new(ui_event_channel.1, tocks_event_channel.0);

    futures::select! {
        _ = tocks.run().fuse() => {},
        event_server_result = event_server.run().fuse() => {
            if let Err(e) = event_server_result {
                error!("Event server died {}", e);
            }
        }
    }
}
