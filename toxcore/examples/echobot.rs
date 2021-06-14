use structopt::StructOpt;
use toxcore::{Event, SaveData};

use futures::{channel::mpsc, prelude::*};

#[derive(Debug, StructOpt)]
enum Options {
    New {},
    Load { key: String },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    pretty_env_logger::init();

    let opt = Options::from_args();

    let (event_tx, mut event_rx) = mpsc::unbounded();

    let message_tx = event_tx.clone();

    let builder = toxcore::Tox::builder()?
        .log(true)
        .event_callback(move |event| {
            let _ = message_tx.unbounded_send(event);
        });

    let mut tox = match opt {
        Options::New {} => {
            let tox = builder.build()?;
            println!("secret_key: {}", tox.self_secret_key());
            tox
        }
        Options::Load { key } => {
            let key = hex::decode(key).unwrap();
            let data = SaveData::SecretKey(key);
            builder.savedata(data).build()?
        }
    };

    println!("address: {}", tox.self_address());

    loop {
        let event = {
            futures::select! {
                event = event_rx.next().fuse() => {
                    Some(event.unwrap())
                }
                _ = tox.run().fuse() => None,
            }
        };

        match event {
            Some(Event::FriendRequest(request)) => {
                tox.add_friend_norequest(&request.public_key).unwrap();
            }
            Some(Event::MessageReceived(friend, message)) => {
                tox.send_message(&friend, &message).unwrap();
            }
            _ => {}
        }
    }
}
