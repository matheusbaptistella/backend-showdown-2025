use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
    ops::Bound::{Included, Unbounded},
};

use chrono::{DateTime, Utc};
use tokio::sync::Notify;


#[derive(Clone, Default)]
pub struct Inflight{
    index: Arc<Mutex<BTreeMap<i64, u64>>>,
    notify:  Arc<Notify>,
}

impl Inflight {
    pub async fn start(&self, timestamp: i64) {
        let mut state = self.index.lock().unwrap();
        *state.entry(timestamp).or_insert(0) += 1;
    }

    pub async fn end(&self, timestamp: i64) {
        let mut state = self.index.lock().unwrap();
        let c = state.get_mut(&timestamp).unwrap();
        *c -= 1;
        if *c == 0 {
            state.remove(&timestamp);
            self.notify.notify_waiters();
        }
    }

    pub fn is_locked(&self, from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>) -> bool {
        if let None = to {
            return false;
        }

        let start_bound = from.map(|ts| Included(ts.timestamp_micros())).unwrap_or(Unbounded);
        let end_bound = to.map(|ts| Included(ts.timestamp_micros())).unwrap_or(Unbounded);

        let state = self.index.lock().unwrap();
        state.range((start_bound, end_bound)).any(|(_, &c)| c > 0)
    }

    pub async fn wait_until_unlocked(&self, from: Option<DateTime<Utc>>, to: Option<DateTime<Utc>>) {
        while self.is_locked(from, to) {
            self.notify.notified().await;
        }
    }
}
