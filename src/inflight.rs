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

pub struct InflightGuard<'a> {
    inflight: &'a Inflight,
    timestamp: i64,
}

impl Inflight {
    pub fn register(&self, timestamp: i64) -> InflightGuard {
        let mut state = self.index.lock().unwrap();
        *state.entry(timestamp).or_insert(0) += 1;

        InflightGuard { inflight: self, timestamp }
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

impl<'a> Drop for InflightGuard<'a> {
    fn drop(&mut self) {
        let mut state = self.inflight.index.lock().unwrap();
        let c = state.get_mut(&self.timestamp).unwrap();
        *c -= 1;
        if *c == 0 {
            state.remove(&self.timestamp);
            self.inflight.notify.notify_waiters();
        }
    }
}
