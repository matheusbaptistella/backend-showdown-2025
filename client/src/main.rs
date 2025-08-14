use std::{sync::Arc, time::Duration};

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

use tracing::Instrument;
use tracing_subscriber::FmtSubscriber;

#[derive(Clone, Copy, Debug)]
enum Processor { Default, Fallback }

#[derive(Clone, Debug)]
struct AppState {
    queue_tx: mpsc::Sender<RequestPayment>,
    concurrency: Arc<Semaphore>,
    http: reqwest::Client,
    db: BufferedClient,
}


#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel::<RequestPayment>(4096);

    let db_client = database::Client::connect(&format!("database:{}", DEFAULT_PORT)).await.unwrap();

    let db = database::BufferedClient::buffer(db_client);

    let appstate = AppState {
        queue_tx: tx.clone(),
        concurrency: Arc::new(Semaphore::new(16)),
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
            process_payment(job, Processor::Default, &st).await;
        });
    }
}

async fn process_payment(job: RequestPayment, processor: Processor, state: &AppState) {
    let (mut bucket, mut url) = match processor {
        Processor::Default => (b'0', "http://payment-processor-default:8080/payments"),
        Processor::Fallback => (b'1', "http://payment-processor-fallback:8080/payments"),
    };
    let mut retry = false;

    loop {
        let resp = state.http
            .post(url)
            .json(&job)
            .send()
            .await
            .unwrap();

        if resp.status().is_server_error() {
            retry = true;
            (bucket, url) = match processor {
                Processor::Default => (b'1', "http://payment-processor-fallback:8080/payments"),
                Processor::Fallback => ( b'0', "http://payment-processor-default:8080/payments"),
            };

            continue;
        }

        if resp.status().is_client_error() {
            if retry {
                break;
            }
            
            retry = true;
            (bucket, url) = match processor {
                Processor::Default => (b'1', "http://payment-processor-fallback:8080/payments"),
                Processor::Fallback => ( b'0', "http://payment-processor-default:8080/payments"),
            };

            continue;
        }

        if resp.status().is_success() {
            let timestamp = job.requested_at.timestamp_millis();
            let amount = (job.amount * 100.0) as u64;

            state.db.set(bucket, timestamp, amount).await;
        }

        break;
    }
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

