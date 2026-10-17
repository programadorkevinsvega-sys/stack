use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Wallet {
    pub user_id: Uuid,
    pub balance: Decimal,
    pub reserved_balance: Decimal,
    pub currency: String,
    pub low_balance_threshold: Decimal,
    pub updated_at: DateTime<Utc>,
}

impl Wallet {
    pub fn available_balance(&self) -> Decimal {
        self.balance - self.reserved_balance
    }
}

#[derive(Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct WalletLedgerEntry {
    pub id: Uuid,
    pub user_id: Uuid,
    pub entry_type: String,
    pub amount: Decimal,
    pub balance_after: Decimal,
    pub reference_type: String,
    pub reference_id: Option<Uuid>,
    pub idempotency_key: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct BillingTransaction {
    pub id: Uuid,
    pub user_id: Uuid,
    #[sqlx(rename = "type")]
    pub transaction_type: String,
    pub amount: Decimal,
    pub currency: String,
    pub provider: Option<String>,
    pub provider_reference: Option<String>,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct Reservation {
    pub id: Uuid,
    pub user_id: Uuid,
    pub amount: Decimal,
    pub currency: String,
    pub status: String,
    pub idempotency_key: String,
    pub request_id: Option<Uuid>,
    pub model_catalog_id: Option<Uuid>,
    pub actual_amount: Option<Decimal>,
    pub aws_cost: Option<Decimal>,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub released_at: Option<DateTime<Utc>>,
}
