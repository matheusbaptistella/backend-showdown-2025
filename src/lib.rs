use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod db;
pub use db::Db;

pub enum Processor {
    Default,
    Fallback,
}

#[derive(Deserialize, Serialize)]
pub struct PaymentPayload {
    #[serde(rename = "correlationId")]
    pub correlation_id: String,
    pub amount: f64,
}

#[derive(Clone, Serialize)]
pub struct Payment {
    #[serde(rename = "correlationId")]
    pub correlation_id: String,
    pub amount: f64,
    #[serde(rename = "requestedAt")]
    pub requested_at: DateTime<Utc>,
}

#[derive(Clone, Deserialize, Serialize)]
pub struct SummaryQueryParams {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub only_local: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ProcessorSummaries {
    #[serde(rename = "default")]
    pub default_sum: Summary,
    pub fallback: Summary,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct Summary {
    #[serde(rename = "totalRequests")]
    pub total_requests: u64,
    #[serde(rename = "totalAmount")]
    pub total_amount: f64,
}
