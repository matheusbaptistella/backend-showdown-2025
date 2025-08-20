use axum::{
    Json, Router,
    extract::{Query, State},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;
use client_full::{
    CreatePayment, Db, DbHandle, PaymentProcessorsSummaries, PaymentsSummaryQueryParams,
    RequestPayment, Summary,
};
use std::sync::Arc;

use tokio::sync::{Semaphore, mpsc};

enum Processor {
    Default,
    Fallback,
}

#[derive(Clone)]
struct AppState {
    queue_tx: mpsc::Sender<RequestPayment>,
    concurrency: Arc<Semaphore>,
    http: reqwest::Client,
    default_db: DbHandle,
    fallback_db: DbHandle,
    peer_url: String,
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel::<RequestPayment>(8192);

    let default_db = DbHandle::new(Db::new());
    let fallback_db = DbHandle::new(Db::new());

    let peer_url = std::env::var("PEER_URL").ok().unwrap();

    let appstate = AppState {
        queue_tx: tx.clone(),
        concurrency: Arc::new(Semaphore::new(16)),
        http: reqwest::Client::builder()
            .tcp_nodelay(true)
            .build()
            .unwrap(),
        default_db,
        fallback_db,
        peer_url,
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

async fn run_dispatcher(mut rx: mpsc::Receiver<RequestPayment>, state: AppState) {
    while let Some(job) = rx.recv().await {
        let permit = match state.concurrency.clone().acquire_owned().await {
            Ok(p) => p,
            Err(_) => break,
        };

        let st = state.clone();
        tokio::spawn(async move {
            let _permit = permit;
            process_payment(job, &st).await;
        });
    }
}

async fn process_payment(job: RequestPayment, state: &AppState) {
    let mut processor = Processor::Default;
    let mut id_retry = false;

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

async fn summarize_local(
    appstate: &AppState,
    params: &PaymentsSummaryQueryParams,
) -> PaymentProcessorsSummaries {
    let from = params.from.map(|dt| dt.timestamp_micros());
    let to = params.to.map(|dt| dt.timestamp_micros());

    let (d_count, d_total) = appstate.default_db.get(from, to).await;
    let (f_count, f_total) = appstate.fallback_db.get(from, to).await;

    let default_sum = Summary {
        total_requests: d_count,
        total_amount: d_total as f64 / 100.0,
    };

    let fallback = Summary {
        total_requests: f_count,
        total_amount: f_total as f64 / 100.0,
    };

    PaymentProcessorsSummaries {
        default_sum,
        fallback,
    }
}

async fn payments(State(appstate): State<AppState>, Json(cp): Json<CreatePayment>) {
    let rp = RequestPayment {
        correlation_id: cp.correlation_id,
        amount: cp.amount,
        requested_at: Utc::now(),
    };

    appstate.queue_tx.send(rp).await.unwrap();
}

async fn payments_summary_local(
    State(appstate): State<AppState>,
    Query(params): Query<PaymentsSummaryQueryParams>,
) -> impl IntoResponse {
    Json(summarize_local(&appstate, &params).await)
}

async fn payments_summary(
    State(appstate): State<AppState>,
    Query(params): Query<PaymentsSummaryQueryParams>,
) -> impl IntoResponse {
    let mut total = summarize_local(&appstate, &params).await;

    let endpoint = format!(
        "{}/payments-summary/local",
        appstate.peer_url.trim_end_matches('/')
    );

    let resp = appstate
        .http
        .get(endpoint)
        .query(&params)
        .send()
        .await
        .unwrap();

    let data = resp.json::<PaymentProcessorsSummaries>().await.unwrap();

    total.default_sum.total_amount += data.default_sum.total_amount;
    total.default_sum.total_requests += data.default_sum.total_requests;
    total.fallback.total_amount += data.fallback.total_amount;
    total.fallback.total_requests += data.fallback.total_requests;

    Json(total)
}
