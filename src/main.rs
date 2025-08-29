use std::sync::Arc;
use axum::{
    Json, Router,
    extract::{Query, State},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use client_full::{Db, Payment, PaymentPayload, Processor, ProcessorSummaries, Summary, SummaryQueryParams};
use reqwest::StatusCode;
use tokio::sync::{mpsc, Semaphore};

#[derive(Clone)]
struct AppState {
    req_queue_tx: mpsc::Sender<(Payment, u64)>,
    default_db: Db,
    fallback_db: Db,
    http: reqwest::Client,
    peer_url: String,
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel::<(Payment, u64)>(10240);
    let app_state = AppState {
        req_queue_tx: tx.clone(),
        default_db: Db::default(),
        fallback_db: Db::default(),
        http: reqwest::Client::builder()
            .tcp_nodelay(true)
            .build()
            .unwrap(),
        peer_url: std::env::var("PEER_URL").ok().unwrap(),
    };

    tokio::spawn(dispatcher(rx, app_state.clone()));

    let app = Router::new()
        .route("/payments", post(payments))
        .route("/payments-summary", get(payments_summary))
        .with_state(app_state);
    
    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("Listening on 0.0.0.0:3000");

    axum::serve(listener, app).await.unwrap();
}

async fn dispatcher(
    mut rx: mpsc::Receiver<(Payment, u64)>,
    app_state: AppState,
) {
    let concurrency = Arc::new(Semaphore::new(100));

    while let Some((p, retries)) = rx.recv().await {
        let permit = concurrency.clone().acquire_owned().await.unwrap();
        let task_state = app_state.clone();

        tokio::spawn(async move {
            let _permit = permit;
            process_payment(p, retries, &task_state).await;
        });
    }
}

async fn process_payment(p: Payment, retries: u64, task_state: &AppState) {
    let mut processor = Processor::Default;

    if retries % 2 != 0 {
        processor = Processor::Fallback;
    }

    let url = match processor {
        Processor::Default => "http://payment-processor-default:8080/payments",
        Processor::Fallback => "http://payment-processor-fallback:8080/payments",
    };
    let status = task_state.http.post(url).json(&p).send().await.unwrap().status();

    if status.is_success() {
        let timestamp = p.requested_at.timestamp_micros();
        let amount = (p.amount * 100.0) as u64;

        match processor {
            Processor::Default => task_state.default_db.set(timestamp, amount),
            Processor::Fallback => task_state.fallback_db.set(timestamp, amount),
        }
    } else if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        let retries = retries + 1;

        task_state.req_queue_tx.send((p, retries)).await.unwrap();
    }
}

async fn payments(State(app_state): State<AppState>, Json(payload): Json<PaymentPayload>) {
    let p = Payment {
        correlation_id: payload.correlation_id,
        amount: payload.amount,
        requested_at: Utc::now(),
    };

    app_state.req_queue_tx.send((p, 0)).await.unwrap();
}

async fn payments_summary(
    State(app_state): State<AppState>,
    Query(params): Query<SummaryQueryParams>,
) -> impl IntoResponse {
    let mut total = local_summary(&app_state, params.from, params.to);

    if let None = params.only_local {
        let remote_data = remote_summary(&app_state, params.from, params.to).await;

        total.default_sum.total_amount += remote_data.default_sum.total_amount;
        total.default_sum.total_requests += remote_data.default_sum.total_requests;
        total.fallback.total_amount += remote_data.fallback.total_amount;
        total.fallback.total_requests += remote_data.fallback.total_requests;
    }

    Json(total)
}

fn local_summary(
    app_state: &AppState,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> ProcessorSummaries {
    let from = from.map(|dt| dt.timestamp_micros());
    let to = to.map(|dt| dt.timestamp_micros());

    let (d_count, d_total) = app_state.default_db.get(from, to);
    let (f_count, f_total) = app_state.fallback_db.get(from, to);

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
    app_state: &AppState,
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
) -> ProcessorSummaries {
    let endpoint = format!(
        "{}/payments-summary",
        app_state.peer_url.trim_end_matches('/')
    );
    let params = SummaryQueryParams {
        from,
        to,
        only_local: Some(true),
    };
    let resp = app_state.http.get(endpoint).query(&params).send().await.unwrap();

    resp.json::<ProcessorSummaries>().await.unwrap()
}
