use std::{
    collections::BTreeMap,
    ops::Bound::{Included, Unbounded},
};
use tokio::sync::{mpsc, oneshot};

pub enum Command {
    Get(Option<i64>, Option<i64>, oneshot::Sender<(u64, u64)>),
    Set(i64, u64),
}

#[derive(Clone)]
pub struct DbHandle {
    tx: mpsc::Sender<Command>,
}

pub struct Db {
    // Stores the pair (request_count, total_amount) sorted by timestamp in micro seconds
    data: BTreeMap<i64, (u64, u64)>,
}

impl Db {
    pub fn new() -> Db {
        Db {
            data: BTreeMap::new(),
        }
    }

    pub fn get(&self, from: Option<i64>, to: Option<i64>) -> (u64, u64) {
        let start_bound = from.map(Included).unwrap_or(Unbounded);
        let end_bound = to.map(Included).unwrap_or(Unbounded);

        self.data
            .range((start_bound, end_bound))
            .fold((0, 0), |acc, (_ts, (count, sum))| {
                (acc.0 + count, acc.1 + sum)
            })
    }

    pub fn set(&mut self, timestamp: i64, amount: u64) {
        let entry = self.data.entry(timestamp).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += amount;
    }
}

impl DbHandle {
    pub fn new(mut db: Db) -> DbHandle {
        let (tx, mut rx) = mpsc::channel(32);

        tokio::spawn(async move {
            while let Some(cmd) = rx.recv().await {
                match cmd {
                    Command::Get(from, to, tx) => {
                        let result = db.get(from, to);
                        tx.send(result).unwrap();
                    }
                    Command::Set(timestamp, amount) => {
                        db.set(timestamp, amount);
                    }
                }
            }
        });

        DbHandle { tx }
    }

    pub async fn get(&self, from: Option<i64>, to: Option<i64>) -> (u64, u64) {
        let (tx, rx) = oneshot::channel();

        self.tx.send(Command::Get(from, to, tx)).await.unwrap();

        rx.await.ok().unwrap()
    }

    pub async fn set(&self, timestamp: i64, amount: u64) {
        self.tx.send(Command::Set(timestamp, amount)).await.unwrap();
    }
}
