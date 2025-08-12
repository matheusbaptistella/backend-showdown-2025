use std::sync::Arc;

use backend_showdown_2025::{
    CreatePayment, PaymentProcessorsSummaries, PaymentsSummaryQueryParams, RequestPayment, Summary, Command
};

use axum::{
    Json, Router,
    extract::{Query, State},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;

use database::{BufferedClient, DEFAULT_PORT};

use tokio::sync::mpsc::Sender;

#[derive(Clone)]
struct AppState {
    db_handle: BufferedClient,
    requests_handle: Sender<RequestPayment>,
}


#[tokio::main]
async fn main() {
    let (req_tx, req_rx) = tokio::sync::mpsc::channel(1024);

    let db_client = database::Client::connect(&format!("127.0.0.1:{}", DEFAULT_PORT)).await.unwrap();

    let db = database::BufferedClient::buffer(db_client);

    let appstate = AppState {
        db_handle: db,
        requests_handle: req_tx,
    };

    let app = Router::new()
        .route("/payments", post(payments))
        .route("/payments-summary", get(payments_summary))
        .with_state(appstate);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("Listening on 0.0.0.0:3000");
    
    axum::serve(listener, app).await.unwrap();
}


async fn payments(
    State(appstate): State<AppState>,
    Json(cp): Json<CreatePayment>,
) {
    let tx = appstate.requests_handle.clone();

    let rp = RequestPayment {
        correlation_id: cp.correlation_id,
        amount: cp.amount,
        requested_at: Utc::now(),
    };

    tx.send(rp).await.unwrap();
}


async fn payments_summary(
    State(appstate): State<AppState>,
    Query(params): Query<PaymentsSummaryQueryParams>,
) {
    
}
