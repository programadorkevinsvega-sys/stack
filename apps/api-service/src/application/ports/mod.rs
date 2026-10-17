use crate::domain::errors::CatalogError;
use async_trait::async_trait;
use serde_json::Value;
use std::net::IpAddr;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub enum AuditActorType {
    Admin,
    System,
}

impl AuditActorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuditContext {
    pub actor_type: AuditActorType,
    pub actor_id: Option<Uuid>,
    pub ip_address: Option<IpAddr>,
}

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub actor_type: AuditActorType,
    pub actor_id: Option<Uuid>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub metadata: Value,
    pub ip_address: Option<IpAddr>,
}

#[async_trait]
pub trait AuditLogger: Send + Sync {
    async fn record(&self, entry: AuditEntry) -> Result<(), CatalogError>;
}
