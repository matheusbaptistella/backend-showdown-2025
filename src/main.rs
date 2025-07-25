use axum::{
    extract::{Query, State}, response::IntoResponse, routing::{get, post}, Json, Router
};
use chrono::{DateTime, Utc};
use redis::{AsyncCommands, aio::MultiplexedConnection};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};

#[derive(Clone)]
struct AppState {
    redis: MultiplexedConnection,
    client: Client,
}

#[tokio::main]
async fn main() {
    let client = Client::new();
    let redis_client = redis::Client::open("redis://127.0.0.1/").unwrap();
    let mut conn = redis_client
        .get_multiplexed_async_connection()
        .await
        .unwrap();
    let appstate = AppState {
        redis: conn,
        client: client,
    };

    let app = Router::new()
        .route("/payments", post(payments))
        .route("/payments-summary", get(payments_summary))
        .with_state(appstate);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080").await.unwrap();

    axum::serve(listener, app).await.unwrap();
}

async fn payments(State(appstate): State<AppState>, Json(cp): Json<CreatePayment>) -> impl IntoResponse {
    let rp = RequestPayment {
        create_payment: cp,
        requested_at: Utc::now(),
    };

    let response = appstate
        .client
        .clone()
        .post("http://payment-processor-default:8080/payments")
        .json(&rp)
        .send()
        .await;

    if let Err(_) = response {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    let timestamp = rp.requested_at.timestamp_millis();

    let result = appstate
        .redis
        .clone()
        .zadd::<&str, i64, f64, u8>(
            "all",
            rp.create_payment.amount,
            timestamp,
        )
        .await;

    if let Err(_) = result {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    StatusCode::OK
}

async fn payments_summary(
    State(appstate): State<AppState>,
    Query(params): Query<PaymentsSummaryQueryParams>,
) -> impl IntoResponse {
    let start = params.from.map(|dt| dt.timestamp_millis()).unwrap_or_else(|| 0);
    let end = params.to.map(|dt| dt.timestamp_millis()).unwrap_or_else(|| i64::MAX);

    let result = appstate
        .redis
        .clone()
        .zrangebyscore(
            "all",
            start,
            end
        )
        .await;

    if let Err(_) = result {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    let sum = result.iter().filter_map(|s| s.parse::<f64>().ok()).sum();

    let summary = PaymentProcessorsSummaries {
        default_sum: Summary {
            total_requests: 0,
            total_amount: 0.0,
        },
        fallback: Summary {
            total_requests: 0,
            total_amount: 0.0,
        },
    };

    Json(summary)
}

#[derive(Deserialize, Serialize)]
struct CreatePayment {
    #[serde(rename = "correlationId")]
    correlation_id: String,
    amount: f64,
}

#[derive(Serialize)]
struct RequestPayment {
    #[serde(flatten)]
    create_payment: CreatePayment,
    #[serde(rename = "requestedAt")]
    requested_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct PaymentsSummaryQueryParams {
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct PaymentProcessorsSummaries {
    #[serde(rename = "default")]
    default_sum: Summary,
    fallback: Summary,
}

#[derive(Serialize)]
struct Summary {
    #[serde(rename = "totalRequests")]
    total_requests: u32,
    #[serde(rename = "totalAmount")]
    total_amount: f64,
}
