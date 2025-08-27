use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub mod db;
pub use db::{Db};

pub enum Processor {
    Default,
    Fallback,
}

pub type Message = (RequestPayment, u8);

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

#[derive(Clone, Deserialize, Serialize)]
pub struct PaymentsSummaryQueryParams {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct PaymentProcessorsSummaries {
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

#[derive(Debug, Deserialize, Serialize)]
pub struct LocalSummaryParams {
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
    pub requested_at: i64,
}
