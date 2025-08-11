use crate::clients::Client;
use crate::Result;

use bytes::Bytes;
use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot;

enum Command {
    Get(Option<i64>, Option<i64>),
    Set(i64, u64),
}

// Message type sent over the channel to the connection task.
//
// `Command` is the command to forward to the connection.
//
// `oneshot::Sender` is a channel type that sends a **single** value. It is used
// here to send the response received from the connection back to the original
// requester.
type Message = (Command, oneshot::Sender<Result<Option<(u64, u64)>>>);

async fn run(mut client: Client, mut rx: Receiver<Message>) {
    // Repeatedly pop messages from the channel. A return value of `None`
    // indicates that all `BufferedClient` handles have dropped and there will never be
    // another message sent on the channel.
    while let Some((cmd, tx)) = rx.recv().await {
        let response = match cmd {
            Command::Get(from, to) => client.get(from, to).await,
            Command::Set(timestamp, amount) => client.set(timestamp, amount).await.map(|_| None),
        };

        // Send the response back to the caller.
        //
        // Failing to send the message indicates the `rx` half dropped
        // before receiving the message. This is a normal runtime event.
        let _ = tx.send(response);
    }
}

#[derive(Clone)]
pub struct BufferedClient {
    tx: Sender<Message>,
}

impl BufferedClient {
    pub fn buffer(client: Client) -> BufferedClient {
        let (tx, rx) = channel(32);

        tokio::spawn(async move { run(client, rx).await });

        BufferedClient { tx }
    }

    pub async fn get(&mut self, from: Option<i64>, to: Option<i64>) -> Result<Option<(u64, u64)>> {
        // Initialize a new `Get` command to send via the channel.
        let get = Command::Get(from, to);

        // Initialize a new oneshot to be used to receive the response back from the connection.
        let (tx, rx) = oneshot::channel();

        // Send the request
        self.tx.send((get, tx)).await?;

        // Await the response
        match rx.await {
            Ok(res) => res,
            Err(err) => Err(err.into()),
        }
    }

    pub async fn set(&mut self, timestamp: i64, amount: u64) -> Result<()> {
        // Initialize a new `Set` command to send via the channel.
        let set = Command::Set(timestamp, amount);

        // Initialize a new oneshot to be used to receive the response back from the connection.
        let (tx, rx) = oneshot::channel();

        // Send the request
        self.tx.send((set, tx)).await?;

        // Await the response
        match rx.await {
            Ok(res) => res.map(|_| ()),
            Err(err) => Err(err.into()),
        }
    }
}