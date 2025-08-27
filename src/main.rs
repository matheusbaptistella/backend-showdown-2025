use axum::{
    Json, Router,
    extract::{Query, State},
    response::IntoResponse,
    routing::{get, post},
};
use chrono::Utc;
use client_full::{
    CreatePayment, Db, LocalSummaryParams, Message, PaymentProcessorsSummaries, PaymentsSummaryQueryParams, Processor, RequestPayment, Summary
};
use reqwest::StatusCode;
use std::sync::Arc;
use tokio::sync::{mpsc, Semaphore};

#[derive(Clone)]
struct AppState {
    queue_tx: mpsc::Sender<Message>,
    concurrency: Arc<Semaphore>,
    http: reqwest::Client,
    default_db: Db,
    fallback_db: Db,
    peer_url: String,
}

#[tokio::main]
async fn main() {
    let (tx, rx) = mpsc::channel::<Message>(10240);

    let default_db = Db::default();
    let fallback_db = Db::default();

    let peer_url = std::env::var("PEER_URL").ok().unwrap();

    let appstate = AppState {
        queue_tx: tx.clone(),
        concurrency: Arc::new(Semaphore::new(100)),
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

async fn run_dispatcher(mut rx: mpsc::Receiver<Message>, state: AppState) {
    while let Some((rp, retries)) = rx.recv().await {
        let permit = state.concurrency.clone().acquire_owned().await.unwrap();
        let st = state.clone();

        tokio::spawn(async move {
            let _permit = permit;
            process_payment(rp, retries, &st).await;
        });
    }

}

async fn process_payment(rp: RequestPayment, retries: u8, state: &AppState) {
    // let mut processor = Processor::Default;
    // let mut id_retry = false;

    // loop {
    //     let url = match processor {
    //         Processor::Default => "http://payment-processor-default:8080/payments",
    //         Processor::Fallback => "http://payment-processor-fallback:8080/payments",
    //     };

    //     let status = state
    //         .http
    //         .post(url)
    //         .json(&rp)
    //         .send()
    //         .await
    //         .unwrap()
    //         .status();

    //     if status.is_success() {
    //         let timestamp = rp.requested_at.timestamp_micros();
    //         let amount = (rp.amount * 100.0) as u64;

    //         match processor {
    //             Processor::Default => state.default_db.set(timestamp, amount),
    //             Processor::Fallback => state.fallback_db.set(timestamp, amount),
    //         }

    //         break;
    //     } else if status.is_client_error() {
    //         // if id_retry {
    //         //     break;
    //         // }
    //         // id_retry = true;
    //         break;
    //     }

    //     processor = match processor {
    //         Processor::Default => Processor::Fallback,
    //         _ => Processor::Default,
    //     };
    // }Default

    let mut processor = Processor::Default;

    if retries % 2 != 0 {
        processor = Processor::Fallback;
    }

    let url = match processor {
        Processor::Default => "http://payment-processor-default:8080/payments",
        Processor::Fallback => "http://payment-processor-fallback:8080/payments",
    };

    let status = state
        .http
        .post(url)
        .json(&rp)
        .send()
        .await
        .unwrap()
        .status();

    if status.is_success() {
        let timestamp = rp.requested_at.timestamp_micros();
        let amount = (rp.amount * 100.0) as u64;

        match processor {
            Processor::Default => state.default_db.set(timestamp, amount),
            Processor::Fallback => state.fallback_db.set(timestamp, amount),
        }
    } else if status.is_server_error() || status == StatusCode::TOO_MANY_REQUESTS {
        let retries = retries + 1;
        state.queue_tx.send((rp, retries)).await.unwrap();
    }
}

async fn process_remote_summary(state: &AppState, params: &LocalSummaryParams) -> PaymentProcessorsSummaries {
    let endpoint = format!(
        "{}/payments-summary/local",
        state.peer_url.trim_end_matches('/')
    );

    let resp = state
        .http
        .get(endpoint)
        .query(params)
        .send()
        .await
        .unwrap();

    let data = resp.json::<PaymentProcessorsSummaries>().await.unwrap();

    data
}

fn summarize_local(
    state: &AppState,
    params: &PaymentsSummaryQueryParams,
) ->  PaymentProcessorsSummaries{
    let from = params.from.map(|dt| dt.timestamp_micros());
    let to = params.to.map(|dt| dt.timestamp_micros());

    let (d_count, d_total) = state.default_db.get(from, to);
    let (f_count, f_total) = state.fallback_db.get(from, to);

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

    data
}

async fn payments(State(appstate): State<AppState>, Json(cp): Json<CreatePayment>) {
    let rp = RequestPayment {
        correlation_id: cp.correlation_id,
        amount: cp.amount,
        requested_at: Utc::now(),
    };

    appstate.queue_tx.send((rp, 0)).await.unwrap();
}

async fn payments_summary_local(
    State(appstate): State<AppState>,
    Query(params): Query<LocalSummaryParams>,
) -> impl IntoResponse {

    let ps = PaymentsSummaryQueryParams {
        from: params.from,
        to: params.to,
    };

    let data = summarize_local(&appstate, &ps);

    Json(data)
}

async fn payments_summary(
    State(appstate): State<AppState>,
    Query(params): Query<PaymentsSummaryQueryParams>,
) -> impl IntoResponse {
    let requested_at = Utc::now().timestamp_micros();
    let mut total = summarize_local(&appstate, &params);
    let ps = LocalSummaryParams {
        from: params.from,
        to: params.to,
        requested_at,
    };
    let data = process_remote_summary(&appstate, &ps).await;

    total.default_sum.total_amount += data.default_sum.total_amount;
    total.default_sum.total_requests += data.default_sum.total_requests;
    total.fallback.total_amount += data.fallback.total_amount;
    total.fallback.total_requests += data.fallback.total_requests;

    Json(total)
}
