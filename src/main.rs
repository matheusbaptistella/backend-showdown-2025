use axum::{
    Json, Router,
    extract::{Query, State},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;
use client_full::{Command, Payment, PaymentPayload, SummaryQueryParams, Worker};
use tokio::sync::{mpsc, oneshot};


#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel::<Command>(10240);
    let worker_state = Worker::new(tx.clone(), rx);

    for _ in 0..32 {
        let worker = worker_state.clone();

        tokio::spawn(async move {
            worker.run().await;
        });
    }

    let app = Router::new()
        .route("/payments", post(payments))
        .route("/payments-summary", get(payments_summary))
        .with_state(tx.clone());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("Listening on 0.0.0.0:3000");

    axum::serve(listener, app).await.unwrap();
}

async fn payments(State(cmd_queue): State<mpsc::Sender<Command>>, Json(payload): Json<PaymentPayload>) {
    let p = Payment {
        correlation_id: payload.correlation_id,
        amount: payload.amount,
        requested_at: Utc::now(),
    };

    cmd_queue.send(Command::Set(p)).await.unwrap();
}

async fn payments_summary(
    State(cmd_queue): State<mpsc::Sender<Command>>,
    Query(params): Query<SummaryQueryParams>,
) -> impl IntoResponse {
    let (tx, rx) = oneshot::channel();

    cmd_queue
        .send(Command::Get(params.from, params.to, params.only_local, tx))
        .await
        .unwrap();

    Json(rx.await.unwrap())
}
