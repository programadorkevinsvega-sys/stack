use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use uuid::Uuid;

pub mod jetstream;
pub mod outbox;

// ── Event type constants ──────────────────────────────────────────────

pub const EVENT_USAGE_RECORDED: &str = "usage.recorded";
pub const EVENT_PAYMENT_SUCCEEDED: &str = "payment.succeeded";
pub const EVENT_PAYMENT_FAILED: &str = "payment.failed";
pub const EVENT_ACCOUNT_MARKED_UNHEALTHY: &str = "account.marked_unhealthy";
pub const EVENT_ACCOUNT_RECOVERED: &str = "account.recovered";
pub const EVENT_WALLET_DEBITED: &str = "wallet.debited";
pub const EVENT_WALLET_LOW_BALANCE: &str = "wallet.low_balance";
pub const EVENT_BILLING_PERIOD_CLOSED: &str = "billing.period_closed";
pub const EVENT_SUBSCRIPTION_CHANGED: &str = "subscription.changed";
pub const EVENT_API_KEY_REVOKED: &str = "api_key.revoked";
pub const EVENT_API_KEY_AUTHORIZATION_CHANGED: &str = "api_key.authorization_changed";
pub const EVENT_REQUEST_FAILED: &str = "request.failed";
pub const EVENT_FILE_INGEST_REQUESTED: &str = "file.ingest.requested";
pub const EVENT_FILE_VECTORS_DELETE_REQUESTED: &str = "file.vectors.delete.requested";

pub const ALL_EVENT_SUBJECTS: &[&str] = &[
    EVENT_USAGE_RECORDED,
    EVENT_PAYMENT_SUCCEEDED,
    EVENT_PAYMENT_FAILED,
    EVENT_ACCOUNT_MARKED_UNHEALTHY,
    EVENT_ACCOUNT_RECOVERED,
    EVENT_WALLET_DEBITED,
    EVENT_WALLET_LOW_BALANCE,
    EVENT_BILLING_PERIOD_CLOSED,
    EVENT_SUBSCRIPTION_CHANGED,
    EVENT_API_KEY_REVOKED,
    EVENT_API_KEY_AUTHORIZATION_CHANGED,
    EVENT_REQUEST_FAILED,
    EVENT_FILE_INGEST_REQUESTED,
    EVENT_FILE_VECTORS_DELETE_REQUESTED,
];

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EventType {
    #[serde(rename = "usage.recorded")]
    UsageRecorded,
    #[serde(rename = "payment.succeeded")]
    PaymentSucceeded,
    #[serde(rename = "payment.failed")]
    PaymentFailed,
    #[serde(rename = "account.marked_unhealthy")]
    AccountMarkedUnhealthy,
    #[serde(rename = "account.recovered")]
    AccountRecovered,
    #[serde(rename = "wallet.debited")]
    WalletDebited,
    #[serde(rename = "wallet.low_balance")]
    WalletLowBalance,
    #[serde(rename = "billing.period_closed")]
    BillingPeriodClosed,
    #[serde(rename = "subscription.changed")]
    SubscriptionChanged,
    #[serde(rename = "api_key.revoked")]
    ApiKeyRevoked,
    ApiKeyAuthorizationChanged,
    #[serde(rename = "request.failed")]
    RequestFailed,
    #[serde(rename = "file.ingest.requested")]
    FileIngestRequested,
    #[serde(rename = "file.vectors.delete.requested")]
    FileVectorsDeleteRequested,
}

impl std::fmt::Display for EventType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            EventType::UsageRecorded => "usage.recorded",
            EventType::PaymentSucceeded => "payment.succeeded",
            EventType::PaymentFailed => "payment.failed",
            EventType::AccountMarkedUnhealthy => "account.marked_unhealthy",
            EventType::AccountRecovered => "account.recovered",
            EventType::WalletDebited => "wallet.debited",
            EventType::WalletLowBalance => "wallet.low_balance",
            EventType::BillingPeriodClosed => "billing.period_closed",
            EventType::SubscriptionChanged => "subscription.changed",
            EventType::ApiKeyRevoked => "api_key.revoked",
            EventType::ApiKeyAuthorizationChanged => "api_key.authorization_changed",
            EventType::RequestFailed => "request.failed",
            EventType::FileIngestRequested => "file.ingest.requested",
            EventType::FileVectorsDeleteRequested => "file.vectors.delete.requested",
        };
        write!(f, "{s}")
    }
}

impl From<&str> for EventType {
    fn from(value: &str) -> Self {
        match value {
            "usage.recorded" => EventType::UsageRecorded,
            "payment.succeeded" => EventType::PaymentSucceeded,
            "payment.failed" => EventType::PaymentFailed,
            "account.marked_unhealthy" => EventType::AccountMarkedUnhealthy,
            "account.recovered" => EventType::AccountRecovered,
            "wallet.debited" => EventType::WalletDebited,
            "wallet.low_balance" => EventType::WalletLowBalance,
            "billing.period_closed" => EventType::BillingPeriodClosed,
            "subscription.changed" => EventType::SubscriptionChanged,
            "api_key.revoked" => EventType::ApiKeyRevoked,
            "api_key.authorization_changed" => EventType::ApiKeyAuthorizationChanged,
            "request.failed" => EventType::RequestFailed,
            "file.ingest.requested" => EventType::FileIngestRequested,
            "file.vectors.delete.requested" => EventType::FileVectorsDeleteRequested,
            _ => panic!("unknown event type: {value}"),
        }
    }
}

// ── Envelope canónico ─────────────────────────────────────────────────

/// Envoltorio inmutable para todo evento NATS.
/// Garantiza que publisher y consumer nunca se desincronicen de esquema.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope<T: Serialize> {
    pub event_id: Uuid,
    pub event_type: EventType,
    pub schema_version: u32,
    pub occurred_at: DateTime<Utc>,
    pub trace_id: Option<Uuid>,
    pub span_id: Option<Uuid>,
    pub request_id: Option<Uuid>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub trace_context: BTreeMap<String, String>,
    #[serde(bound(deserialize = "T: Deserialize<'de>"))]
    pub payload: T,
}

impl<T: Serialize> EventEnvelope<T> {
    pub fn new(event_type: impl Into<EventType>, payload: T) -> Self {
        Self {
            event_id: Uuid::now_v7(),
            event_type: event_type.into(),
            schema_version: 1,
            occurred_at: Utc::now(),
            trace_id: None,
            span_id: None,
            request_id: None,
            trace_context: common::observability::current_trace_context(),
            payload,
        }
    }

    pub fn with_trace(
        event_type: impl Into<EventType>,
        payload: T,
        trace_id: Option<Uuid>,
        span_id: Option<Uuid>,
    ) -> Self {
        Self {
            event_id: Uuid::now_v7(),
            event_type: event_type.into(),
            schema_version: 1,
            occurred_at: Utc::now(),
            trace_id,
            span_id,
            request_id: None,
            trace_context: common::observability::current_trace_context(),
            payload,
        }
    }

    pub fn with_request_id(event_type: impl Into<EventType>, payload: T, request_id: Uuid) -> Self {
        Self {
            event_id: Uuid::now_v7(),
            event_type: event_type.into(),
            schema_version: 1,
            occurred_at: Utc::now(),
            trace_id: None,
            span_id: None,
            request_id: Some(request_id),
            trace_context: common::observability::current_trace_context(),
            payload,
        }
    }

    pub fn to_json(&self) -> Result<Vec<u8>, serde_json::Error>
    where
        T: Serialize,
    {
        serde_json::to_vec(self)
    }

    pub fn from_json(data: &[u8]) -> Result<Self, serde_json::Error>
    where
        T: serde::de::DeserializeOwned,
    {
        serde_json::from_slice(data)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileIngestRequested {
    pub file_id: Uuid,
    pub user_id: Uuid,
    pub object_key: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub strategy_version: String,
    pub embedding_model: String,
    #[serde(default)]
    pub quota_reservation_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileVectorsDeleteRequested {
    pub file_id: Uuid,
    pub user_id: Uuid,
    pub reason: String,
}

// ── Event Payloads ────────────────────────────────────────────────────

/// Se publica al terminar cada request a Bedrock (éxito o error).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecorded {
    pub request_id: Uuid,
    pub api_key_id: Uuid,
    pub user_id: Uuid,
    pub model_catalog_id: Uuid,
    pub aws_account_id: Uuid,
    pub input_tokens: u32,
    pub output_tokens: u32,
    pub cached_tokens: u32,
    pub stream: bool,
    pub latency_ms: u32,
    pub ttft_ms: Option<u32>,
    pub status_code: i32,
    pub error_code: Option<String>,
}

/// Recarga o cobro exitoso desde la pasarela de pago.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentSucceeded {
    pub transaction_id: Uuid,
    pub user_id: Uuid,
    pub amount: Decimal,
    pub currency: String,
    pub provider: String,
    pub provider_reference: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

/// Pago fallido en la pasarela.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaymentFailed {
    pub transaction_id: Uuid,
    pub user_id: Uuid,
    pub amount: Decimal,
    pub currency: String,
    pub provider: String,
    pub reason: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub metadata: BTreeMap<String, String>,
}

/// Circuit breaker disparado para una cuenta AWS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountMarkedUnhealthy {
    pub account_id: Uuid,
    pub aws_account_id: String,
    pub error_code: String,
    pub error_message: String,
}

/// Circuit breaker recuperado para una cuenta AWS.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountRecovered {
    pub account_id: Uuid,
    pub aws_account_id: String,
}

/// Débito aplicado al wallet de un usuario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletDebited {
    pub user_id: Uuid,
    pub amount: Decimal,
    pub currency: String,
    pub balance_after: Decimal,
    pub usage_event_id: Uuid,
}

/// Alerta de saldo bajo.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletLowBalance {
    pub user_id: Uuid,
    pub currency: String,
    pub current_balance: Decimal,
    pub threshold: Decimal,
}

/// Cierre de período de facturación (postpago).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BillingPeriodClosed {
    pub user_id: Uuid,
    pub subscription_id: Uuid,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_usage_cost: Decimal,
    pub currency: String,
    pub invoice_id: Uuid,
}

/// Cambio de plan de suscripción.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubscriptionChanged {
    pub user_id: Uuid,
    pub old_plan_id: Uuid,
    pub new_plan_id: Uuid,
    pub changed_at: DateTime<Utc>,
}

/// API key revocada — usado para invalidar cache en caliente.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRevoked {
    pub api_key_id: Uuid,
    pub user_id: Uuid,
    pub revoked_by: Uuid,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyAuthorizationChanged {
    pub api_key_id: Uuid,
    pub user_id: Uuid,
    pub changed_at: DateTime<Utc>,
    pub reason: String,
}

/// Request fallido para telemetría.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RequestFailed {
    pub request_id: Uuid,
    pub api_key_id: Uuid,
    pub user_id: Uuid,
    pub error_code: String,
    pub error_message: String,
    pub aws_account_id: Option<Uuid>,
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_envelope_roundtrip() {
        let payload = UsageRecorded {
            request_id: Uuid::now_v7(),
            api_key_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            model_catalog_id: Uuid::new_v4(),
            aws_account_id: Uuid::new_v4(),
            input_tokens: 150,
            output_tokens: 300,
            cached_tokens: 0,
            stream: false,
            latency_ms: 450,
            ttft_ms: None,
            status_code: 200,
            error_code: None,
        };

        let envelope = EventEnvelope::new(EVENT_USAGE_RECORDED, payload);
        let json = envelope.to_json().unwrap();
        let decoded: EventEnvelope<UsageRecorded> = EventEnvelope::from_json(&json).unwrap();

        assert_eq!(envelope.event_id, decoded.event_id);
        assert_eq!(decoded.event_type, EventType::UsageRecorded);
        assert_eq!(decoded.schema_version, 1);
        assert!(decoded.trace_id.is_none());
        assert!(decoded.span_id.is_none());
        assert_eq!(decoded.payload.input_tokens, 150);
        assert_eq!(decoded.payload.output_tokens, 300);
    }

    #[test]
    fn test_envelope_with_trace() {
        let trace_id = Uuid::now_v7();
        let span_id = Uuid::now_v7();
        let payload = PaymentSucceeded {
            transaction_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            amount: Decimal::new(1000, 2),
            currency: "USD".into(),
            provider: "stripe".into(),
            provider_reference: "pi_123".into(),
            kind: String::new(),
            metadata: BTreeMap::new(),
        };
        let envelope = EventEnvelope::with_trace(
            EVENT_PAYMENT_SUCCEEDED,
            payload,
            Some(trace_id),
            Some(span_id),
        );
        let json = envelope.to_json().unwrap();
        let decoded: EventEnvelope<PaymentSucceeded> = EventEnvelope::from_json(&json).unwrap();

        assert_eq!(decoded.trace_id, Some(trace_id));
        assert_eq!(decoded.span_id, Some(span_id));
        assert_eq!(decoded.schema_version, 1);
    }

    #[test]
    fn test_envelope_schema_version_is_1() {
        let payload = PaymentSucceeded {
            transaction_id: Uuid::new_v4(),
            user_id: Uuid::new_v4(),
            amount: Decimal::new(1000, 2),
            currency: "USD".into(),
            provider: "stripe".into(),
            provider_reference: "pi_123".into(),
            kind: String::new(),
            metadata: BTreeMap::new(),
        };
        let envelope = EventEnvelope::new(EVENT_PAYMENT_SUCCEEDED, payload);
        assert_eq!(envelope.schema_version, 1);
    }
}
