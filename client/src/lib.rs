use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub enum Command {
    Get(Option<i64>, Option<i64>),
    Set(i64, u64),
}

#[derive(Deserialize, Serialize)]
pub struct CreatePayment {
    #[serde(rename = "correlationId")]
    pub correlation_id: String,
    pub amount: f64,
}

#[derive(Serialize)]
pub struct RequestPayment {
    #[serde(rename = "correlationId")]
    pub correlation_id: String,
    pub amount: f64,
    #[serde(rename = "requestedAt")]
    pub requested_at: DateTime<Utc>,
}

#[derive(Deserialize)]
pub struct PaymentsSummaryQueryParams {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
pub struct PaymentProcessorsSummaries {
    #[serde(rename = "default")]
    pub default_sum: Summary,
    pub fallback: Summary,
}

#[derive(Serialize)]
pub struct Summary {
    #[serde(rename = "totalRequests")]
    pub total_requests: usize,
    #[serde(rename = "totalAmount")]
    pub total_amount: f64,
}
