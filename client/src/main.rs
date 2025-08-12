use std::sync::Arc;

use client::{
    CreatePayment, PaymentProcessorsSummaries, PaymentsSummaryQueryParams, RequestPayment, Summary
};

use axum::{
    Json, Router,
    extract::{Query, State},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;

use database::{BufferedClient, DEFAULT_PORT};

use tokio::sync::{mpsc, Semaphore};

#[derive(Clone)]
struct AppState {
    queue_tx: mpsc::Sender<RequestPayment>,
    concurrency: Arc<Semaphore>,
    http: reqwest::Client,
    db: BufferedClient,
}


#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel::<RequestPayment>(1024);

    let db_client = database::Client::connect(&format!("127.0.0.1:{}", DEFAULT_PORT)).await.unwrap();

    let db = database::BufferedClient::buffer(db_client);

    let appstate = AppState {
        queue_tx: tx.clone(),
        concurrency: Arc::new(Semaphore::new(200)),
        http: reqwest::Client::builder()
            .tcp_nodelay(true)
            .build()
            .unwrap(),
        db: db,
    };

    let dispatcher_state = appstate.clone();
    tokio::spawn(async move {
        run_dispatcher(rx, dispatcher_state).await
    });

    let app = Router::new()
        .route("/payments", post(payments))
        .route("/payments-summary", get(payments_summary))
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
    let _ = state.http
        .post("http://payment-processor-default:8080/payments")
        .json(&job)
        .send()
        .await
        .unwrap();

    let timestamp = job.requested_at.timestamp_millis();
    let amount = (job.amount * 100.0) as u64;

    state.db.set(b'0', timestamp, amount).await.unwrap();
}


async fn payments(
    State(appstate): State<AppState>,
    Json(cp): Json<CreatePayment>,
) {
    let rp = RequestPayment {
        correlation_id: cp.correlation_id,
        amount: cp.amount,
        requested_at: Utc::now(),
    };

    appstate.queue_tx.send(rp).await.unwrap();
}


async fn payments_summary(
    State(appstate): State<AppState>,
    Query(params): Query<PaymentsSummaryQueryParams>,
) -> impl IntoResponse {
    let from = params.from.map(|dt| dt.timestamp_millis());
    
    let to = params.to.map(|dt| dt.timestamp_millis());

    let ((d_count, d_total), (f_count, f_total))  = appstate.db.get(from, to).await.unwrap().unwrap();

    let default_sum = Summary {
        total_requests: d_count,
        total_amount: d_total as f64 / 100.0,
    };

    let fallback = Summary {
        total_requests: f_count,
        total_amount: f_total as f64 / 100.0,
    };

    Json(PaymentProcessorsSummaries {
        default_sum,
        fallback,
    })
}

