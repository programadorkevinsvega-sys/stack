use crate::domain::entities::{
    CapturedPayment, ProviderOrder, ProviderPaymentToken, ProviderVaultSetup,
};
use crate::domain::errors::PaymentError;
use async_trait::async_trait;
use rust_decimal::Decimal;
use serde_json::Value;
use uuid::Uuid;

pub struct CreateOrderInput {
    pub user_id: Uuid,
    pub amount: Decimal,
    pub currency: String,
    pub idempotency_key: String,
    pub return_url: String,
    pub cancel_url: String,
}

pub struct PayPalWebhookHeaders {
    pub transmission_id: String,
    pub transmission_time: String,
    pub cert_url: String,
    pub auth_algo: String,
    pub transmission_sig: String,
}

#[async_trait]
pub trait PaymentProvider: Send + Sync {
    async fn create_order(&self, input: CreateOrderInput) -> Result<ProviderOrder, PaymentError>;
    async fn capture_order(&self, order_id: &str) -> Result<CapturedPayment, PaymentError>;
    async fn charge_vault_token(
        &self,
        user_id: Uuid,
        payment_token_id: &str,
        amount: Decimal,
        currency: &str,
        idempotency_key: &str,
    ) -> Result<CapturedPayment, PaymentError>;
    async fn create_vault_setup_token(
        &self,
        user_id: Uuid,
        return_url: String,
        cancel_url: String,
    ) -> Result<ProviderVaultSetup, PaymentError>;
    async fn create_payment_token(
        &self,
        user_id: Uuid,
        setup_token_id: &str,
    ) -> Result<ProviderPaymentToken, PaymentError>;
    async fn verify_webhook(
        &self,
        headers: &PayPalWebhookHeaders,
        event: &Value,
    ) -> Result<(), PaymentError>;
}

pub struct AuditLogInput {
    pub actor_type: String,
    pub actor_id: Option<Uuid>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub metadata: Value,
    pub ip_address: Option<String>,
}

#[async_trait]
pub trait AuditLogger: Send + Sync {
    async fn log(&self, input: AuditLogInput) -> Result<(), PaymentError>;
}
