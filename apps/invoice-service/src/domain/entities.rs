use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Invoice {
    pub id: Uuid,
    pub user_id: Uuid,
    pub subscription_id: Option<Uuid>,
    pub invoice_number: String,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub subtotal: Decimal,
    pub tax: Decimal,
    pub total: Decimal,
    pub currency: String,
    pub status: String,
    pub due_date: Option<DateTime<Utc>>,
    pub paid_at: Option<DateTime<Utc>>,
    pub pdf_url: Option<String>,
    pub created_at: DateTime<Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct InvoiceLineItem {
    pub id: Uuid,
    pub invoice_id: Uuid,
    pub description: String,
    pub quantity: Decimal,
    pub unit_price: Decimal,
    pub amount: Decimal,
}

#[derive(Clone, Debug)]
pub struct InvoiceDocument {
    pub invoice: Invoice,
    pub line_items: Vec<InvoiceLineItem>,
}

#[derive(Clone, Debug, sqlx::FromRow)]
pub struct UsageLineDraft {
    pub model_catalog_id: Uuid,
    pub request_count: i64,
    pub token_count: Decimal,
}

#[derive(Clone, Debug)]
pub struct LineItemDraft {
    pub description: String,
    pub quantity: Decimal,
    pub unit_price: Decimal,
    pub amount: Decimal,
}
