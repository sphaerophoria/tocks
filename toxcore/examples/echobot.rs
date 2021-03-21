use structopt::StructOpt;
use toxcore::{Friend, Message, PublicKey, SaveData};

use tokio::sync::mpsc;

#[derive(Debug, StructOpt)]
enum Options {
    New {},
    Load { key: String },
}

enum Event {
    IncomingMessage(Friend, Message),
    IncomingFriendRequest(PublicKey),
    None,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let opt = Options::from_args();

    let (event_tx, mut event_rx) = mpsc::unbounded_channel();

    let message_tx = event_tx.clone();

    let builder = toxcore::Tox::builder()?
        .log(true)
        .friend_message_callback(move |friend, message| {
            let _ = message_tx.send(Event::IncomingMessage(friend, message));
        })
        .friend_request_callback(move |request| {
            let _ = event_tx.send(Event::IncomingFriendRequest(request.public_key));
        });

    let mut tox = match opt {
        Options::New {} => {
            let tox = builder.build()?;
            println!("secret_key: {}", tox.self_secret_key());
            tox
        }
        Options::Load { key } => {
            let key = hex::decode(key).unwrap();
            let data = SaveData::SecretKey(&key);
            builder.savedata(data).build()?
        }
    };

    println!("address: {}", tox.self_address());

    loop {
        let event = {
            tokio::select! {
                event = event_rx.recv() => {
                    event.unwrap()
                }
                _ = tox.run() => { Event::None }
            }
        };

        match event {
            Event::IncomingFriendRequest(public_key) => {
                tox.add_friend_norequest(&public_key).unwrap();
            }
            Event::IncomingMessage(friend, message) => {
                tox.send_message(&friend, &message).unwrap();
            }
            Event::None => {}
        }
    }
}
