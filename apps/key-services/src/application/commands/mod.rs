use crate::application::ports::{
    ApiKeyCache, ApiKeyPlanLimits, AuditLogger, KeyCreationRateLimiter, KeyGenerator,
};
use crate::domain::entities::{ApiKey, ApiKeyScope, RateLimitOverride};
use crate::domain::errors::KeyServiceError;
use crate::domain::repositories::ApiKeyRepository;
use chrono::{DateTime, Utc};
use common::AuditEntry;
use serde_json::json;
use std::net::IpAddr;
use std::sync::Arc;
use uuid::Uuid;

pub const MAX_ACTIVE_KEYS_PER_USER: i64 = 20;

pub fn max_active_keys_per_user() -> i64 {
    std::env::var("KEY_SERVICES_MAX_ACTIVE_KEYS_PER_USER")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(MAX_ACTIVE_KEYS_PER_USER)
}

#[derive(Debug, Clone)]
pub struct CreateApiKeyInput {
    pub user_id: Uuid,
    pub name: String,
    pub scopes: Vec<ApiKeyScope>,
    pub allowed_ips: Vec<IpAddr>,
    pub rate_limit_override: RateLimitOverride,
    pub expires_at: Option<DateTime<Utc>>,
}

pub struct CreateApiKey {
    pub repo: Arc<dyn ApiKeyRepository>,
    pub generator: Arc<dyn KeyGenerator>,
    pub rate_limiter: Arc<dyn KeyCreationRateLimiter>,
    pub audit_logger: Arc<dyn AuditLogger>,
    pub plan_limits: Arc<dyn ApiKeyPlanLimits>,
}
impl CreateApiKey {
    pub async fn execute(
        &self,
        input: CreateApiKeyInput,
    ) -> Result<(ApiKey, String), KeyServiceError> {
        self.execute_inner(input, true).await
    }

    async fn execute_inner(
        &self,
        input: CreateApiKeyInput,
        enforce_creation_rate: bool,
    ) -> Result<(ApiKey, String), KeyServiceError> {
        if input.name.trim().is_empty() {
            return Err(KeyServiceError::Validation("name is required".into()));
        }
        if input.scopes.is_empty() {
            return Err(KeyServiceError::Validation(
                "at least one scope is required".into(),
            ));
        }
        input.rate_limit_override.validate()?;
        if input.expires_at.is_some_and(|value| value <= Utc::now()) {
            return Err(KeyServiceError::Validation(
                "expires_at must be in the future".into(),
            ));
        }
        if enforce_creation_rate {
            self.rate_limiter.check(input.user_id).await?;
        }
        let plan_max = self.plan_limits.max_api_keys(input.user_id).await?;
        // Subscription semantics use 0 for unlimited. A local fallback is
        // retained only for deployments that explicitly disable dynamic
        // limits; the production path always has a plan provider.
        let max = if plan_max > 0 {
            plan_max
        } else if std::env::var("KEY_SERVICES_DISABLE_DYNAMIC_PLAN_LIMITS").is_ok() {
            max_active_keys_per_user()
        } else {
            0
        };
        let generated = self.generator.generate()?;
        let now = Utc::now();
        let mut scopes = input.scopes;
        scopes.sort();
        scopes.dedup();
        let key = ApiKey {
            id: Uuid::now_v7(),
            user_id: input.user_id,
            name: input.name.trim().to_string(),
            key_prefix: generated.prefix,
            key_hash: generated.hash,
            scopes,
            allowed_ips: input.allowed_ips,
            rate_limit_override: input.rate_limit_override,
            status: "active".into(),
            expires_at: input.expires_at,
            last_used_at: None,
            last_used_ip: None,
            revoked_at: None,
            revoked_reason: None,
            created_at: now,
        };
        if !self.repo.save_if_capacity(&key, max).await? {
            return Err(KeyServiceError::MaxKeysPerUserReached);
        }

        if let Err(e) = self
            .audit_logger
            .record(AuditEntry {
                actor_type: "system".into(),
                actor_id: None,
                action: "api_key.created".into(),
                resource_type: "api_key".into(),
                resource_id: Some(key.id),
                metadata: json!({"name": key.name, "scopes": key.scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>()}),
                ip_address: None,
            })
            .await
        {
            tracing::warn!(error = %e, key_id = %key.id, "Failed to record audit log for key creation");
        }

        Ok((key, generated.plaintext))
    }
}

pub struct RevokeApiKey {
    pub repo: Arc<dyn ApiKeyRepository>,
    pub cache: Arc<dyn ApiKeyCache>,
    pub audit_logger: Arc<dyn AuditLogger>,
}
impl RevokeApiKey {
    pub async fn execute(
        &self,
        key_id: Uuid,
        user_id: Uuid,
        revoked_by: Uuid,
        reason: String,
    ) -> Result<(), KeyServiceError> {
        let key = self
            .repo
            .find_by_id(key_id)
            .await?
            .ok_or(KeyServiceError::ApiKeyNotFound)?;
        if key.user_id != user_id {
            return Err(KeyServiceError::ApiKeyNotFound);
        }
        if key.status == "revoked" {
            self.cache.invalidate(&key.key_hash).await?;
            return Ok(());
        }
        if reason.trim().is_empty() || reason.chars().count() > 200 {
            return Err(KeyServiceError::Validation(
                "revocation reason must contain 1 to 200 characters".into(),
            ));
        }
        self.repo.revoke(key_id, revoked_by, &reason).await?;
        self.cache.invalidate(&key.key_hash).await?;

        if let Err(e) = self
            .audit_logger
            .record(AuditEntry {
                actor_type: "system".into(),
                actor_id: Some(revoked_by),
                action: "api_key.revoked".into(),
                resource_type: "api_key".into(),
                resource_id: Some(key_id),
                metadata: json!({"name": key.name, "reason": reason}),
                ip_address: None,
            })
            .await
        {
            tracing::warn!(error = %e, key_id = %key_id, "Failed to record audit log for key revocation");
        }

        Ok(())
    }
}

pub struct RotateApiKey {
    pub create: CreateApiKey,
    pub revoke: RevokeApiKey,
}
impl RotateApiKey {
    pub async fn execute(
        &self,
        key_id: Uuid,
        user_id: Uuid,
        revoked_by: Uuid,
        reason: String,
        scopes: Option<Vec<ApiKeyScope>>,
    ) -> Result<(ApiKey, String), KeyServiceError> {
        let old = self
            .revoke
            .repo
            .find_by_id(key_id)
            .await?
            .ok_or(KeyServiceError::ApiKeyNotFound)?;
        if old.user_id != user_id {
            return Err(KeyServiceError::ApiKeyNotFound);
        }
        // Resolve the subscription before revoking the old key so an
        // unavailable subscription service cannot leave rotation half-done.
        self.create.plan_limits.max_api_keys(user_id).await?;
        let input = CreateApiKeyInput {
            user_id,
            name: old.name.clone(),
            scopes: scopes.unwrap_or_else(|| old.scopes.clone()),
            allowed_ips: old.allowed_ips.clone(),
            rate_limit_override: old.rate_limit_override.clone(),
            expires_at: old.expires_at,
        };
        self.create.rate_limiter.check_rotation(user_id).await?;
        self.revoke
            .execute(key_id, user_id, revoked_by, reason)
            .await?;
        let created = self.create.execute_inner(input, false).await?;
        Ok(created)
    }
}

pub struct UpdateKeyMetadata {
    pub repo: Arc<dyn ApiKeyRepository>,
    pub cache: Arc<dyn ApiKeyCache>,
    pub audit_logger: Arc<dyn AuditLogger>,
}
impl UpdateKeyMetadata {
    pub async fn execute(
        &self,
        key_id: Uuid,
        user_id: Uuid,
        name: String,
        allowed_ips: Vec<IpAddr>,
        rate_limit_override: RateLimitOverride,
        scopes: Option<Vec<ApiKeyScope>>,
    ) -> Result<ApiKey, KeyServiceError> {
        if name.trim().is_empty() {
            return Err(KeyServiceError::Validation("name is required".into()));
        }
        rate_limit_override.validate()?;
        let mut key = self
            .repo
            .find_by_id(key_id)
            .await?
            .ok_or(KeyServiceError::ApiKeyNotFound)?;
        if key.user_id != user_id {
            return Err(KeyServiceError::ApiKeyNotFound);
        }
        key.name = name.trim().to_string();
        key.allowed_ips = allowed_ips;
        key.rate_limit_override = rate_limit_override;
        if let Some(mut values) = scopes {
            if values.is_empty() {
                return Err(KeyServiceError::Validation(
                    "at least one scope is required".into(),
                ));
            }
            values.sort();
            values.dedup();
            key.scopes = values;
        }
        self.repo.update(&key).await?;
        self.cache.invalidate(&key.key_hash).await?;

        if let Err(e) = self
            .audit_logger
            .record(AuditEntry {
                actor_type: "system".into(),
                actor_id: None,
                action: "api_key.updated".into(),
                resource_type: "api_key".into(),
                resource_id: Some(key_id),
                metadata: json!({
                    "name": key.name,
                    "scopes": key.scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>()
                }),
                ip_address: None,
            })
            .await
        {
            tracing::warn!(error = %e, key_id = %key_id, "Failed to record audit log for key update");
        }

        Ok(key)
    }
}
