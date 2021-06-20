use tocks::{EventClient, TocksUiEvent};
use toxcore::ToxId;

use futures::prelude::*;
use structopt::StructOpt;

use std::str::FromStr;

#[derive(StructOpt)]
enum WriteCommand {
    Close,
    CreateAccount {
        name: String,
        password: String,
    },
    AcceptPendingFriend {
        account: i64,
        user: i64,
    },
    RequestFriend {
        account: i64,
        tox_id: String,
        message: String,
    },
    BlockUser {
        account: i64,
        user: i64,
    },
    PurgeUser {
        account: i64,
        user: i64,
    },
    Login {
        account_name: String,
        password: String,
    },
    SendMessage {
        account: i64,
        chat: i64,
        message: String,
    },
    LoadMessages {
        account: i64,
        chat: i64,
    },
}

#[derive(StructOpt)]
enum Opts {
    Read,
    Write {
        #[structopt(subcommand)]
        command: WriteCommand,
    },
    Raw {
        command: String,
    },
}

#[tokio::main]
async fn main() {
    let client = EventClient::connect().await.unwrap();

    let options = Opts::from_args();

    match options {
        Opts::Read => print_events(client).await,
        Opts::Write { command } => send_command(client, parse_command(command)).await,
        Opts::Raw { command } => send_command(client, parse_raw(command)).await,
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

fn parse_raw(command: String) -> TocksUiEvent {
    serde_json::from_str::<TocksUiEvent>(&command).expect("Invalid tocks ui event")
}

fn parse_command(command: WriteCommand) -> TocksUiEvent {
    match command {
        WriteCommand::Close => TocksUiEvent::Close,
        WriteCommand::AcceptPendingFriend { account, user } => {
            TocksUiEvent::AcceptPendingFriend(account.into(), user.into())
        }
        WriteCommand::BlockUser { account, user } => {
            TocksUiEvent::BlockUser(account.into(), user.into())
        }
        WriteCommand::PurgeUser { account, user } => {
            TocksUiEvent::PurgeUser(account.into(), user.into())
        }
        WriteCommand::CreateAccount { name, password } => {
            TocksUiEvent::CreateAccount(name, password)
        }
        WriteCommand::Login {
            account_name,
            password,
        } => TocksUiEvent::Login(account_name, password),
        WriteCommand::LoadMessages { account, chat } => {
            TocksUiEvent::LoadMessages(account.into(), chat.into())
        }
        WriteCommand::RequestFriend {
            account,
            tox_id,
            message,
        } => TocksUiEvent::RequestFriend(
            account.into(),
            ToxId::from_str(&tox_id).expect("Invalid tox id"),
            message,
        ),
        WriteCommand::SendMessage {
            account,
            chat,
            message,
        } => TocksUiEvent::MessageSent(account.into(), chat.into(), message),
    }
}

async fn send_command(mut client: EventClient, event: TocksUiEvent) {
    client.send(event).await.expect("Failed to send event");
}
