use std::{
    collections::BTreeMap,
    ops::Bound::{Included, Unbounded}, sync::{Arc, Mutex},
};

#[derive(Clone, Default)]
pub struct Db {
    // Stores the pair (request_count, total_amount) sorted by timestamp in micro seconds
    data: Arc<Mutex<BTreeMap<i64, (u64, u64)>>>,
}

impl Db {
    pub fn get(&self, from: Option<i64>, to: Option<i64>) -> (u64, u64) {
        let state = self.data.lock().unwrap();
        let start_bound = from.map(Included).unwrap_or(Unbounded);
        let end_bound = to.map(Included).unwrap_or(Unbounded);

        state
            .range((start_bound, end_bound))
            .fold((0, 0), |acc, (_ts, (count, sum))| {
                (acc.0 + count, acc.1 + sum)
            })
    }

    pub fn set(&self, timestamp: i64, amount: u64) {
        let mut state = self.data.lock().unwrap();
        let entry = state.entry(timestamp).or_insert((0, 0));
        entry.0 += 1;
        entry.1 += amount;
    }
}
