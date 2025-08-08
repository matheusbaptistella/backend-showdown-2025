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
    let subscriber = tracing_subscriber::FmtSubscriber::new();
    tracing::subscriber::set_global_default(subscriber).unwrap();

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
    tracing::info!("Listening on 0.0.0.0:3000");

    axum::serve(listener, app).await.unwrap();
}

async fn payments(
    State(appstate): State<AppState>,
    Json(cp): Json<CreatePayment>,
) -> impl IntoResponse {
    let amount = (cp.amount * 100.0) as u64;
    let key = format!("{}:{}", cp.correlation_id, amount);

    let rp = RequestPayment {
        correlation_id: cp.correlation_id,
        amount: cp.amount,
        requested_at: Utc::now(),
    };

    let timestamp = rp.requested_at.timestamp_millis();

    let mut try_fallback = false;
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
                    if resp.status() == StatusCode::UNPROCESSABLE_ENTITY {
                        try_fallback = true;
                    }
                    else {
                        let result = appstate
                        .redis
                        .clone()
                        .zadd::<&str, i64, &str, u8>("default", &key, timestamp)
                        .await;

                        if let Err(e) = result {
                            tracing::info!("redis error {}", e);
                            return StatusCode::INTERNAL_SERVER_ERROR;
                        }
                    }

                    break;
                }
            },
            Err(e) => {
                tracing::info!("Error in default request {}", e);
                return StatusCode::INTERNAL_SERVER_ERROR;
            }
        }
    };

    if try_fallback {
        loop {
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
                        if resp.status().is_success() {
                            let result = appstate
                            .redis
                            .clone()
                            .zadd::<&str, i64, &str, u8>("default", &key, timestamp)
                            .await;

                            if let Err(e) = result {
                                tracing::info!("redis error {}", e);
                                return StatusCode::INTERNAL_SERVER_ERROR;
                            }

                            if let Ok(num) = result {
                                tracing::info!("Redis result = {}", num)
                            }
                        }
                        else {
                            tracing::info!("Status is not success");
                            return StatusCode::INTERNAL_SERVER_ERROR;
                        }

                        break;
                    }
                }
                Err(e) => {
                    tracing::info!("Error in fallback request {}", e);
                    return StatusCode::INTERNAL_SERVER_ERROR;
                }
            }
        };
    };

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

    let parsed: Vec<u64> = result
        .into_iter()
        .filter_map(|s| s.split_once(':').and_then(|(_, a)| a.parse::<u64>().ok()))
        .collect();

    let count = parsed.len();

    let sum: u64 = parsed.iter().sum();

    let sum_trunc: f64 = (sum as f64) / 100.0;

    let default_sum = Summary {
        total_requests: count,
        total_amount: sum_trunc,
    };

    let result = appstate
        .redis
        .clone()
        .zrangebyscore::<&str, i64, i64, Vec<String>>("fallback", start, end)
        .await
        .unwrap();

    let parsed: Vec<u64> = result
        .into_iter()
        .filter_map(|s| s.split_once(':').and_then(|(_, a)| a.parse::<u64>().ok()))
        .collect();

    let count = parsed.len();

    let sum: u64 = parsed.iter().sum();

    let sum_trunc: f64 = (sum as f64) / 100.0;

    let fallback = Summary {
        total_requests: count,
        total_amount: sum_trunc,
    };

    let summary = PaymentProcessorsSummaries {
        default_sum,
        fallback,
    };

    Json(summary)
}
