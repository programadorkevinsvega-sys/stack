use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PaymentMethod {
    pub id: Uuid,
    pub user_id: Uuid,
    pub provider: String,
    pub provider_payment_method_id: String,
    pub brand: Option<String>,
    pub last4: Option<String>,
    pub exp_month: Option<i16>,
    pub exp_year: Option<i16>,
    pub is_default: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PaymentProviderEvent {
    pub id: Uuid,
    pub provider: String,
    pub provider_event_id: String,
    pub event_type: String,
    pub payload: serde_json::Value,
    pub processed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PaymentTransaction {
    pub id: Uuid,
    pub user_id: Uuid,
    pub provider: String,
    pub provider_reference: String,
    pub kind: String,
    pub status: String,
    pub amount: Decimal,
    pub currency: String,
    pub idempotency_key: Option<String>,
    pub failure_reason: Option<String>,
    pub metadata: serde_json::Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Clone)]
pub struct ProviderOrder {
    pub provider_reference: String,
    pub approval_url: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapturedPayment {
    pub provider_reference: String,
    pub order_id: String,
    pub amount: Decimal,
    pub currency: String,
    pub status: String,
}

#[derive(Clone)]
pub struct ProviderVaultSetup {
    pub setup_token_id: String,
    pub approval_url: String,
    pub status: String,
}

#[derive(Clone)]
pub struct ProviderPaymentToken {
    pub payment_token_id: String,
    pub payer_id: Option<String>,
    pub email: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct TopUpContext {
    pub user_id: Uuid,
    pub transaction_id: Uuid,
    pub amount: Decimal,
    pub currency: String,
    pub order_id: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub reference_id: String,
}
