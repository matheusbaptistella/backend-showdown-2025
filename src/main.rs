use axum::{
    Json, Router,
    extract::{Query, State},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;
use client_full::{worker::WorkerState, Command, Payment, PaymentPayload, SummaryQueryParams, Worker};
use tokio::sync::{mpsc, oneshot};


#[tokio::main]
async fn main() {
    let worker_state = WorkerState::new();
    let mut set_workers_txs = Vec::new();

    for _ in 0..64 {
        let (w_tx, w_rx) = mpsc::channel::<Command>(4096);
        set_workers_txs.push(w_tx.clone());

        let worker = Worker::new(w_tx, worker_state.clone());

        tokio::spawn(async move {
            worker.run(w_rx).await
        });
    }

    let mut get_workers_txs = Vec::new();

    for _ in 0..2 {
        let (w_tx, w_rx) = mpsc::channel::<Command>(4096);
        get_workers_txs.push(w_tx.clone());

        let worker = Worker::new(w_tx, worker_state.clone());

        tokio::spawn(async move {
            worker.run(w_rx).await
        });
    }

    let (tx, rx) = mpsc::channel::<Command>(10240);

    tokio::spawn(dispatcher(rx, get_workers_txs, set_workers_txs));

    let app = Router::new()
        .route("/payments", post(payments))
        .route("/payments-summary", get(payments_summary))
        .with_state(tx.clone());

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();

    println!("Listening on 0.0.0.0:3000");

    axum::serve(listener, app).await.unwrap();
}

async fn dispatcher(
    mut rx: mpsc::Receiver<Command>,
    get_workers_txs: Vec<mpsc::Sender<Command>>,
    set_workers_txs: Vec<mpsc::Sender<Command>>,
) {
    let mut i = 0;

    while let Some(cmd) = rx.recv().await {
        match cmd {
            Command::Set(..) => {
                let tx = &set_workers_txs[i % set_workers_txs.len()];
                i += 1;

                tx.send(cmd).await.unwrap();
            },
            Command::Get(..) => {
                let tx = &get_workers_txs[i % get_workers_txs.len()];
                i += 1;

                tx.send(cmd).await.unwrap();
            }
        }

    }
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
