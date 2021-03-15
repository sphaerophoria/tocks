use structopt::StructOpt;
use toxcore::{Friend, Message, PublicKey, SaveData};

use tokio::sync::broadcast::Receiver;

use futures::FutureExt;

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

    let builder = toxcore::Tox::builder()?.log(true);

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

    let mut incoming_message_handles: Vec<(Friend, Receiver<Message>)> = Vec::new();

    let mut friend_requests = tox.friend_requests();
    loop {
        let event = {
            let incoming_message_futures = incoming_message_handles
                .iter_mut()
                .map(|(friend, handle)| {
                    async move {
                        let message = handle.recv().await;
                        message.map(|message| (friend, message))
                    }
                    .boxed()
                })
                .collect::<Vec<_>>();

            let incoming_message_select = if !incoming_message_futures.is_empty() {
                futures::future::select_all(incoming_message_futures.into_iter()).boxed()
            } else {
                futures::future::pending().boxed()
            };

            tokio::select! {
                incoming_message = incoming_message_select => {
                    if let Ok(incoming_message) = incoming_message.0 {
                        Event::IncomingMessage(incoming_message.0.clone(), incoming_message.1)
                    }
                    else {
                        Event::None
                    }
                }
                friend_request = friend_requests.recv() => {
                    if let Ok(request) = friend_request {
                        Event::IncomingFriendRequest(request.public_key)
                    }
                    else {
                        Event::None
                    }
                },
                _ = tox.run() => { Event::None }
            }
        };

        match event {
            Event::IncomingFriendRequest(public_key) => {
                let friend = tox.add_friend_norequest(&public_key).unwrap();
                let incoming_messages = tox.incoming_friend_messages(&friend);
                incoming_message_handles.push((friend, incoming_messages));
            }
            Event::IncomingMessage(friend, message) => {
                tox.send_message(&friend, &message).unwrap();
            }
            Event::None => {}
        }
    }
}
