use axum::{
    Json, Router,
    extract::{Query, State},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;
use client_full::{
    Command, CreatePayment, Db, DbHandle, GetRequest, Inflight, PaymentProcessorsSummaries, PaymentsSummaryQueryParams, Processor, RequestPayment, Summary
};

use tokio::sync::{mpsc, oneshot};

#[derive(Clone)]
struct AppState {
    queue_tx: mpsc::Sender<Command>,
    http: reqwest::Client,
    default_db: DbHandle,
    fallback_db: DbHandle,
    peer_url: String,
    inflight: Inflight,
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel::<Command>(1024);

    let default_db = DbHandle::new(Db::new());
    let fallback_db = DbHandle::new(Db::new());

    let peer_url = std::env::var("PEER_URL").ok().unwrap();

    let appstate = AppState {
        queue_tx: tx.clone(),
        http: reqwest::Client::builder()
            .tcp_nodelay(true)
            .build()
            .unwrap(),
        default_db,
        fallback_db,
        peer_url,
        inflight: Inflight::default(),
    };

    let dispatcher_state = appstate.clone();
    tokio::spawn(async move { run_dispatcher(rx, dispatcher_state).await });

    let app = Router::new()
        .route("/payments", post(payments))
        .route("/payments-summary", get(payments_summary))
        .route("/payments-summary/local", get(payments_summary_local))
        .with_state(appstate);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("Listening on 0.0.0.0:3000");

    axum::serve(listener, app).await.unwrap();
}

async fn run_dispatcher(mut rx: mpsc::Receiver<Command>, state: AppState) {
    while let Some(cmd) = rx.recv().await {
        let st = state.clone();

        match cmd {
            Command::Set(job) => {
                tokio::spawn(async move {
                    process_payment(job, &st).await;
                });
            },
            Command::Get(job) => {
                tokio::spawn(async move {
                    st.inflight.wait_until_unlocked(job.0.from, job.0.to).await;
                    summarize_local(&st, job).await;
                });
            },
            Command::GetRemote(job) => {
                tokio::spawn(async move {
                    st.inflight.wait_until_unlocked(job.0.from, job.0.to).await;
                    process_remote_summary(&st, job).await;
                });
            },
        }
    }
}

async fn process_payment(job: RequestPayment, state: &AppState) {
    let mut processor = Processor::Default;
    let mut id_retry = false;
    let timestamp = job.requested_at.timestamp_micros();
    let _guard = state.inflight.register(timestamp);

    loop {
        let url = match processor {
            Processor::Default => "http://payment-processor-default:8080/payments",
            Processor::Fallback => "http://payment-processor-fallback:8080/payments",
        };

        let status = state
            .http
            .post(url)
            .json(&job)
            .send()
            .await
            .unwrap()
            .status();

        if status.is_success() {
            let timestamp = job.requested_at.timestamp_micros();
            let amount = (job.amount * 100.0) as u64;

            match processor {
                Processor::Default => state.default_db.set(timestamp, amount).await,
                Processor::Fallback => state.fallback_db.set(timestamp, amount).await,
            }

            break;
        } else if status.is_client_error() {
            if id_retry {
                break;
            }
            id_retry = true;
        }

        processor = match processor {
            Processor::Default => Processor::Fallback,
            _ => Processor::Default,
        };
    }
}

async fn process_remote_summary(state: &AppState, job: GetRequest) {
    let endpoint = format!(
        "{}/payments-summary/local",
        state.peer_url.trim_end_matches('/')
    );

    let resp = state
        .http
        .get(endpoint)
        .query(&job.0)
        .send()
        .await
        .unwrap();

    let data = resp.json::<PaymentProcessorsSummaries>().await.unwrap();

    job.1.send(data).unwrap();
}

async fn summarize_local(
    state: &AppState,
    job: GetRequest,
) {
    let from = job.0.from.map(|dt| dt.timestamp_micros());
    let to = job.0.to.map(|dt| dt.timestamp_micros());

    let (d_count, d_total) = state.default_db.get(from, to).await;
    let (f_count, f_total) = state.fallback_db.get(from, to).await;

    let default_sum = Summary {
        total_requests: d_count,
        total_amount: d_total as f64 / 100.0,
    };

    let fallback = Summary {
        total_requests: f_count,
        total_amount: f_total as f64 / 100.0,
    };

    let data = PaymentProcessorsSummaries {
        default_sum,
        fallback,
    };

    job.1.send(data).unwrap();
}

async fn payments(State(appstate): State<AppState>, Json(cp): Json<CreatePayment>) {
    let rp = RequestPayment {
        correlation_id: cp.correlation_id,
        amount: cp.amount,
        requested_at: Utc::now(),
    };

    appstate.queue_tx.send(Command::Set(rp)).await.unwrap();
}

async fn payments_summary_local(
    State(appstate): State<AppState>,
    Query(params): Query<PaymentsSummaryQueryParams>,
) -> impl IntoResponse {
    let (tx, rx) = oneshot::channel::<PaymentProcessorsSummaries>();

    appstate.queue_tx.send(Command::Get((params, tx))).await.unwrap();

    let total = rx.await.unwrap();

    Json(total)
}

async fn payments_summary(
    State(appstate): State<AppState>,
    Query(params): Query<PaymentsSummaryQueryParams>,
) -> impl IntoResponse {
    let (tx, rx) = oneshot::channel::<PaymentProcessorsSummaries>();
    let (r_tx, r_rx) = oneshot::channel::<PaymentProcessorsSummaries>();

    appstate.queue_tx.send(Command::Get((params.clone(), tx))).await.unwrap();

    let mut total = rx.await.unwrap();

    appstate.queue_tx.send(Command::GetRemote((params, r_tx))).await.unwrap();

    let data = r_rx.await.unwrap();

    total.default_sum.total_amount += data.default_sum.total_amount;
    total.default_sum.total_requests += data.default_sum.total_requests;
    total.fallback.total_amount += data.fallback.total_amount;
    total.fallback.total_requests += data.fallback.total_requests;

    Json(total)
}
