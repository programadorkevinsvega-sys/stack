use crate::domain::errors::SubscriptionError;
use async_trait::async_trait;
use common::AuditEntry;

#[async_trait]
pub trait AuditLogger: Send + Sync {
    async fn record(&self, entry: AuditEntry) -> Result<(), SubscriptionError>;
}
