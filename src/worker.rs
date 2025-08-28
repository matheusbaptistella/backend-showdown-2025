use chrono::{DateTime, Utc};
use reqwest::StatusCode;
use tokio::sync::{mpsc, oneshot};

use crate::{Db, Payment, Processor, ProcessorSummaries, Summary, SummaryQueryParams};

pub enum Command {
    Set(Payment),
    Get(
        Option<DateTime<Utc>>,
        Option<DateTime<Utc>>,
        Option<bool>,
        oneshot::Sender<ProcessorSummaries>,
    ),
}

#[derive(Clone)]
pub struct WorkerState {
    default_db: Db,
    fallback_db: Db,
    http: reqwest::Client,
    peer_url: String,
}

#[derive(Clone)]
pub struct Worker {
    cmd_queue_tx: mpsc::Sender<Command>,
    state: WorkerState,
}

impl WorkerState {
    pub fn new() -> Self {
        WorkerState {
            default_db: Db::default(),
            fallback_db: Db::default(),
            http: reqwest::Client::builder()
                .tcp_nodelay(true)
                .build()
                .unwrap(),
            peer_url: std::env::var("PEER_URL").ok().unwrap(),
        }
    }
}

impl Worker {
    pub fn new(tx: mpsc::Sender<Command>, state: WorkerState) -> Self {
        Worker {
            cmd_queue_tx: tx,
            state,
        }
    }

    pub async fn run(self, mut rx: mpsc::Receiver<Command>) {
        let mut fails = 1;

        while let Some(cmd) = rx.recv().await {
            self.process(cmd, &mut fails).await;
        }
    }

    async fn process(&self, cmd: Command, fails: &mut u64) {
        match cmd {
            Command::Get(from, to, only_local, tx) => {
                let mut local_data = self.local_summary(from, to);

                if let None = only_local {
                    let remote_data = self.remote_summary(from, to).await;

                    local_data.default_sum.total_amount += remote_data.default_sum.total_amount;
                    local_data.default_sum.total_requests += remote_data.default_sum.total_requests;
                    local_data.fallback.total_amount += remote_data.fallback.total_amount;
                    local_data.fallback.total_requests += remote_data.fallback.total_requests;
                }

                tx.send(local_data).unwrap();
            }
            Command::Set(p) => {
                let mut processor = Processor::Default;

                if *fails % 4 != 0 {
                    processor = Processor::Fallback;
                }

                let url = match processor {
                    Processor::Default => "http://payment-processor-default:8080/payments",
                    Processor::Fallback => "http://payment-processor-fallback:8080/payments",
                };

                let status = self.state.http.post(url).json(&p).send().await.unwrap().status();

                if status.is_success() {
                    let timestamp = p.requested_at.timestamp_micros();
                    let amount = (p.amount * 100.0) as u64;

                    match processor {
                        Processor::Default => self.state.default_db.set(timestamp, amount),
                        Processor::Fallback => self.state.fallback_db.set(timestamp, amount),
                    }
                } else if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
                    *fails += 1;
                    self.cmd_queue_tx.send(Command::Set(p)).await.unwrap();
                }
            }
        }
    }

    fn local_summary(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> ProcessorSummaries {
        let from = from.map(|dt| dt.timestamp_micros());
        let to = to.map(|dt| dt.timestamp_micros());

        let (d_count, d_total) = self.state.default_db.get(from, to);
        let (f_count, f_total) = self.state.fallback_db.get(from, to);

        let default_sum = Summary {
            total_requests: d_count,
            total_amount: d_total as f64 / 100.0,
        };
        let fallback = Summary {
            total_requests: f_count,
            total_amount: f_total as f64 / 100.0,
        };

        ProcessorSummaries {
            default_sum,
            fallback,
        }
    }

    async fn remote_summary(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> ProcessorSummaries {
        let endpoint = format!(
            "{}/payments-summary",
            self.state.peer_url.trim_end_matches('/')
        );
        let params = SummaryQueryParams {
            from,
            to,
            only_local: Some(true),
        };
        let resp = self.state.http.get(endpoint).query(&params).send().await.unwrap();

        resp.json::<ProcessorSummaries>().await.unwrap()
    }
}
