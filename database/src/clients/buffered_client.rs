use crate::clients::Client;
use crate::Result;

use tokio::sync::mpsc::{channel, Receiver, Sender};
use tokio::sync::oneshot;

enum Command {
    Get(Option<i64>, Option<i64>),
    Set(u8, i64, u64),
}

type Message = (Command, Option<oneshot::Sender<Result<Option<((u64, u64), (u64, u64))>>>>);

async fn run(mut client: Client, mut rx: Receiver<Message>) {
    while let Some((cmd, tx_opt)) = rx.recv().await {
        let response = match cmd {
            Command::Get(from, to) => client.get(from, to).await,
            Command::Set(instance, timestamp, amount) => {
                client.set(instance, timestamp, amount).await.unwrap();
                Ok(None)
            },
        };
        
        if let Some(tx) = tx_opt {
            let _ = tx.send(response);
        }
    }
}

#[derive(Clone)]
pub struct BufferedClient {
    tx: Sender<Message>,
}

impl BufferedClient {
    pub fn buffer(client: Client) -> BufferedClient {
        let (tx, rx) = channel(10240);

        tokio::spawn(async move { run(client, rx).await });

        BufferedClient { tx }
    }

    pub async fn get(&self, from: Option<i64>, to: Option<i64>) -> Result<Option<((u64, u64), (u64, u64))>> {
        let get = Command::Get(from, to);

        // Initialize a new oneshot to be used to receive the response back from the connection.
        let (tx, rx) = oneshot::channel();

        // Send the request
        self.tx.send((get, Some(tx))).await?;

        // Await the response
        match rx.await {
            Ok(res) => res,
            Err(err) => Err(err.into()),
        }
    }

    pub async fn set(&self, instance: u8, timestamp: i64, amount: u64) {
        let _ = self.tx.send((Command::Set(instance, timestamp, amount), None)).await.unwrap();
    }
}