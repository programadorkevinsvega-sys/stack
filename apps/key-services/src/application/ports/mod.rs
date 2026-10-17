use crate::domain::entities::{ApiKey, GeneratedApiKey};
use crate::domain::errors::KeyServiceError;
use async_trait::async_trait;
use common::AuditEntry;
use std::net::IpAddr;
use uuid::Uuid;

#[async_trait]
pub trait AuditLogger: Send + Sync {
    async fn record(&self, entry: AuditEntry) -> Result<(), KeyServiceError>;
}

#[async_trait]
pub trait ApiKeyCache: Send + Sync {
    async fn get(&self, hash: &str) -> Result<Option<ApiKey>, KeyServiceError>;
    async fn set(&self, key: &ApiKey) -> Result<(), KeyServiceError>;
    async fn invalidate(&self, hash: &str) -> Result<(), KeyServiceError>;
    /// Bumps the authorization epoch for a user. Cached entries carry the
    /// epoch and are rejected after a subscription/scope policy change.
    async fn invalidate_user(&self, user_id: Uuid) -> Result<(), KeyServiceError>;
}

#[async_trait]
pub trait ApiKeyPlanLimits: Send + Sync {
    /// Returns the plan limit. `0` means unlimited, matching subscription
    /// service semantics. A missing subscription is represented by an error.
    async fn max_api_keys(&self, user_id: Uuid) -> Result<i64, KeyServiceError>;
}

pub trait KeyGenerator: Send + Sync {
    fn generate(&self) -> Result<GeneratedApiKey, KeyServiceError>;
    fn hash(&self, plaintext: &str) -> Result<String, KeyServiceError>;
}

#[async_trait]
pub trait KeyCreationRateLimiter: Send + Sync {
    async fn check(&self, user_id: Uuid) -> Result<(), KeyServiceError>;
    async fn check_rotation(&self, user_id: Uuid) -> Result<(), KeyServiceError> {
        self.check(user_id).await
    }
}

pub fn validate_source_ip(
    allowed_ips: &[IpAddr],
    source_ip: Option<IpAddr>,
) -> Result<(), KeyServiceError> {
    if allowed_ips.is_empty() || source_ip.is_some_and(|ip| allowed_ips.contains(&ip)) {
        Ok(())
    } else {
        Err(KeyServiceError::IpNotAllowed)
    }
}

pub fn cache_key(hash: &str) -> String {
    format!("keys:validation:{hash}")
}
