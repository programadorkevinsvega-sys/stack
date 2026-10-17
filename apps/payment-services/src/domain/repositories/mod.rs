use crate::domain::entities::{PaymentMethod, PaymentProviderEvent, PaymentTransaction};
use crate::domain::errors::PaymentError;
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait PaymentMethodRepository: Send + Sync {
    async fn save(&self, value: &PaymentMethod) -> Result<(), PaymentError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<PaymentMethod>, PaymentError>;
    async fn find_by_user(&self, user_id: Uuid) -> Result<Vec<PaymentMethod>, PaymentError>;
    async fn set_default(&self, user_id: Uuid, id: Uuid) -> Result<(), PaymentError>;
    async fn delete(&self, user_id: Uuid, id: Uuid) -> Result<(), PaymentError>;
}

#[async_trait]
pub trait PaymentProviderEventRepository: Send + Sync {
    async fn save(&self, value: &PaymentProviderEvent) -> Result<bool, PaymentError>;
    async fn find_by_provider_event_id(
        &self,
        provider: &str,
        provider_event_id: &str,
    ) -> Result<Option<PaymentProviderEvent>, PaymentError>;
    async fn mark_processed(&self, id: Uuid) -> Result<(), PaymentError>;
}

#[async_trait]
pub trait PaymentTransactionRepository: Send + Sync {
    async fn save(&self, value: &PaymentTransaction) -> Result<(), PaymentError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<PaymentTransaction>, PaymentError>;
    async fn find_by_provider_reference(
        &self,
        provider: &str,
        provider_reference: &str,
    ) -> Result<Option<PaymentTransaction>, PaymentError>;
    async fn find_by_idempotency_key(
        &self,
        provider: &str,
        idempotency_key: &str,
    ) -> Result<Option<PaymentTransaction>, PaymentError>;
    async fn update(&self, value: &PaymentTransaction) -> Result<(), PaymentError>;
}
