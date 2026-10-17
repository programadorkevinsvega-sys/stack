use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct UsageEvent {
    pub id: Uuid,
    pub request_id: Uuid,
    pub api_key_id: Uuid,
    pub user_id: Uuid,
    pub model_catalog_id: Uuid,
    pub aws_account_id: Uuid,
    pub input_tokens: i32,
    pub output_tokens: i32,
    pub cached_tokens: i32,
    pub stream: bool,
    pub latency_ms: i32,
    pub ttft_ms: Option<i32>,
    pub status_code: i16,
    pub error_code: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, sqlx::FromRow)]
pub struct UsageSummary {
    pub total_requests: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_cached_tokens: i64,
    pub error_count: i64,
    pub error_rate: f64,
    pub avg_latency_ms: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct TopModel {
    pub model_catalog_id: Uuid,
    pub total_requests: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub avg_latency_ms: f64,
    pub error_count: i64,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct ErrorBucket {
    pub error_code: String,
    pub count: i64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, sqlx::FromRow)]
pub struct ErrorRate {
    pub total_requests: i64,
    pub error_count: i64,
    pub error_rate: f64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, sqlx::FromRow)]
pub struct LatencyStats {
    pub sample_count: i64,
    pub min_latency_ms: f64,
    pub avg_latency_ms: f64,
    pub p50_latency_ms: f64,
    pub p95_latency_ms: f64,
    pub p99_latency_ms: f64,
    pub max_latency_ms: f64,
}
