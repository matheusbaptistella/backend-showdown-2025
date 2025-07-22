use axum::{
    extract::Query,
    routing::{get, post},
    http::StatusCode,
    Json, Router
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(root))
        .route("/payments", post(payments))
        .route("/payments-summary", get(payments_summary));

    let listener = tokio::net::TcpListener::bind("0.0.0.0::1337").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn root() -> &'static str {
    "Hello, World!"
}

async fn payments(Json(_): Json<CreatePayment>) -> StatusCode {
    StatusCode::OK
}


async fn payments_summary(Query(_): Query<PaymentsSummaryQueryParams>) ->Json<PaymentProcessorsSummaries> {
    let summary = PaymentProcessorsSummaries {
        default_sum: Summary { total_requests: 0, total_amount: 0.0 },
        fallback: Summary { total_requests: 0, total_amount: 0.0 }
    };

    Json(summary)
}


#[derive(Deserialize)]
struct CreatePayment {
    #[serde(rename = "correlationId")]
    correlation_id: String,
    amount: f64,
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
    total_amount:f64,
}


#[derive(Deserialize)]
struct PaymentsSummaryQueryParams {
    from: Option<DateTime<Utc>>,
    to: Option<DateTime<Utc>>,
}
