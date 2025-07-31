use axum::{
    Json, Router,
    extract::{Query, State},
    response::IntoResponse,
    routing::{get, post},
};
use backend_showdown_2025::{
    CreatePayment, PaymentProcessorsSummaries, PaymentsSummaryQueryParams, RequestPayment, Summary,
};
use chrono::Utc;
use redis::{AsyncCommands, aio::MultiplexedConnection};
use reqwest::{Client, StatusCode};

#[derive(Clone)]
struct AppState {
    redis: MultiplexedConnection,
    client: Client,
}

#[tokio::main]
async fn main() {
    let client = Client::new();
    let redis_client = redis::Client::open("redis://redis/").unwrap();
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

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    println!("Listening on 0.0.0.0:3000");

    axum::serve(listener, app).await.unwrap();
}

async fn payments(
    State(appstate): State<AppState>,
    Json(cp): Json<CreatePayment>,
) -> impl IntoResponse {
    let key = format!("{}:{}", cp.correlation_id, cp.amount);

    let res = appstate
        .redis
        .clone()
        .zscore::<&str, &str, Option<i64>>("default", &key)
        .await;

    if let Ok(Some(_score)) = res {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    let res = appstate
        .redis
        .clone()
        .zscore::<&str, &str, Option<i64>>("fallback", &key)
        .await;

    if let Ok(Some(_score)) = res {
        return StatusCode::INTERNAL_SERVER_ERROR;
    }

    let rp = RequestPayment {
        create_payment: cp,
        requested_at: Utc::now(),
    };

    let timestamp = rp.requested_at.timestamp_millis();

    loop {
        let response = appstate
            .client
            .clone()
            .post("http://payment-processor-default:8080/payments")
            .json(&rp)
            .send()
            .await;

        match response {
            Ok(resp) => {
                if !resp.status().is_server_error() {
                    let result = appstate
                        .redis
                        .clone()
                        .zadd::<&str, i64, String, u8>("default", key, timestamp)
                        .await;

                    if let Err(_) = result {
                        return StatusCode::INTERNAL_SERVER_ERROR;
                    }

                    break;
                }
            }
            Err(_) => {}
        };

        let response = appstate
            .client
            .clone()
            .post("http://payment-processor-fallback:8080/payments")
            .json(&rp)
            .send()
            .await;

        match response {
            Ok(resp) => {
                if !resp.status().is_server_error() {
                    let result = appstate
                        .redis
                        .clone()
                        .zadd::<&str, i64, String, u8>("fallback", key, timestamp)
                        .await;

                    if let Err(_) = result {
                        return StatusCode::INTERNAL_SERVER_ERROR;
                    }

                    break;
                }
            }
            Err(_) => {}
        };
    }

    StatusCode::OK
}

async fn payments_summary(
    State(appstate): State<AppState>,
    Query(params): Query<PaymentsSummaryQueryParams>,
) -> impl IntoResponse {
    let start = params
        .from
        .map(|dt| dt.timestamp_millis())
        .unwrap_or_else(|| i64::MIN);
    let end = params
        .to
        .map(|dt| dt.timestamp_millis())
        .unwrap_or_else(|| i64::MAX);

    let result = appstate
        .redis
        .clone()
        .zrangebyscore::<&str, i64, i64, Vec<String>>("default", start, end)
        .await
        .unwrap();

    let parsed: Vec<f64> = result
        .into_iter()
        .filter_map(|s| s.split_once(':').and_then(|(_, a)| a.parse::<f64>().ok()))
        .collect();

    let count = parsed.len();

    let sum: f64 = parsed.iter().sum();

    let default_sum = Summary {
        total_requests: count,
        total_amount: sum,
    };

    let result = appstate
        .redis
        .clone()
        .zrangebyscore::<&str, i64, i64, Vec<String>>("fallback", start, end)
        .await
        .unwrap();

    let parsed: Vec<f64> = result
        .into_iter()
        .filter_map(|s| s.split_once(':').and_then(|(_, a)| a.parse::<f64>().ok()))
        .collect();

    let count = parsed.len();

    let sum: f64 = parsed.iter().sum();

    let fallback = Summary {
        total_requests: count,
        total_amount: sum,
    };

    let summary = PaymentProcessorsSummaries {
        default_sum,
        fallback,
    };

    Json(summary)
}
