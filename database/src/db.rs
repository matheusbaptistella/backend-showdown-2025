use std::{collections::BTreeMap, ops::Bound::{Included, Unbounded}, sync::{Arc, Mutex}};


#[derive(Clone)]
pub struct Db {
    shared: Arc<Shared>,
}


struct Shared {
    state: Mutex<State>,
}


struct State {
    // Binary Tree ordered by timestamp to facilitate range search
    // Stores the pair (request_count, total_amount)
    default: BTreeMap<i64, (u64, u64)>,
    fallback: BTreeMap<i64, (u64, u64)>,
}


impl Db {
    pub fn new() -> Db {
        let shared = Arc::new( Shared {
            state: Mutex::new( State {
                default: BTreeMap::new(),
                fallback: BTreeMap::new(),
            }),
        });

        Db { shared }
    }

    pub fn get(&self, from: Option<i64>, to: Option<i64>) -> ((u64, u64), (u64, u64)) {
        let state = self.shared.state.lock().unwrap();

        let start_bound = from.map(Included).unwrap_or(Unbounded);
        let end_bound = to.map(Included).unwrap_or(Unbounded);

        // Range returns an iterator on the entries of the db in the range
        // We create an acumulator for each entry that sums request count and total amount
        let default_values = state.default.range((start_bound, end_bound)).fold((0, 0), |acc, (_ts, (count, sum))| (acc.0 + count, acc.1 + sum));
        let fallback_values = state.fallback.range((start_bound, end_bound)).fold((0, 0), |acc, (_ts, (count, sum))| (acc.0 + count, acc.1 + sum));

        (default_values, fallback_values)
    }

    pub fn set(&self, instance: u8, timestamp: i64, amount: u64) {
        let mut state = self.shared.state.lock().unwrap();
        if instance == b'0' {
            let entry = state.default.entry(timestamp).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += amount;
        }
        else {
            let entry = state.fallback.entry(timestamp).or_insert((0, 0));
            entry.0 += 1;
            entry.1 += amount;
        }

        drop(state);
    }
}
