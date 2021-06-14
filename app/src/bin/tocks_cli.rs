use tocks::{EventClient, TocksUiEvent};

use futures::prelude::*;
use structopt::StructOpt;

#[derive(StructOpt)]
enum Opts {
    Read,
    Write { command: String },
}

#[tokio::main]
async fn main() {
    let client = EventClient::connect().await.unwrap();

    let options = Opts::from_args();

    match options {
        Opts::Read => print_events(client).await,
        Opts::Write { command } => send_command(client, command).await,
    };
}

async fn print_events(mut client: EventClient) {
    while let Some(item) = client.next().await {
        match item {
            Ok(item) => {
                println!("{}", serde_json::to_string(&item).unwrap());
            }
            Err(e) => {
                if let Some(io_err) = e.downcast_ref::<serde_json::error::Error>() {
                    println!("{:?}", io_err);
                }
                println!("Failed to parse event: {:?}", e);
            }
        }
    }
}

async fn send_command(mut client: EventClient, command: String) {
    let event = serde_json::from_str::<TocksUiEvent>(&command).expect("Invalid tocks ui event");

    client.send(event).await.expect("Failed to send event");
}
