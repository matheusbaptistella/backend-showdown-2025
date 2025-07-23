use axum::{
    extract::{Query, State},
    routing::{get, post},
    Json, Router
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use reqwest::Client;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::RwLock;


#[derive(Clone)]
struct AppState {
    client: Client,
    db: Arc<RwLock<HashMap<String, Data>>>,
}


struct Data {
    data: (DateTime<Utc>, f64)
}


#[tokio::main]
async fn main() {
    let client = Client::new();
    let state = Arc::new(AppState {
        client: client,
        db: Arc::new(RwLock::new(HashMap::new())),
    });

    let app = Router::new()
        .route("/payments", post(payments))
        .route("/payments-summary", get(payments_summary))
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:9999").await.unwrap();

    axum::serve(listener, app).await.unwrap();
}


async fn payments(State(state): State<Arc<AppState>>, Json(cp): Json<CreatePayment>) {
    let mp = MakePayment {
        create_payment: cp,
        requested_at: Utc::now(),
    };

    let _ = state.client.clone().post("http://payment-processor-default:8080/payments").json(&mp).send().await.unwrap();
}


async fn payments_summary(Query(_): Query<PaymentsSummaryQueryParams>) -> Json<PaymentProcessorsSummaries> {
    let summary = PaymentProcessorsSummaries {
        default_sum: Summary { total_requests: 0, total_amount: 0.0 },
        fallback: Summary { total_requests: 0, total_amount: 0.0 }
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
struct MakePayment {
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
    total_amount:f64,
}
