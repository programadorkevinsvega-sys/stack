use crate::application::ports::AuditLogger;
use crate::domain::entities::{ApiKey, ApiKeyScope, RateLimitOverride};
use crate::domain::errors::KeyServiceError;
use crate::domain::repositories::ApiKeyRepository;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use common::{AuditEntry, PgAuditLogger};
use events::{
    outbox::enqueue_in_transaction, ApiKeyAuthorizationChanged, ApiKeyRevoked, EventEnvelope,
    EVENT_API_KEY_AUTHORIZATION_CHANGED, EVENT_API_KEY_REVOKED,
};
use serde_json::Value;
use sqlx::{FromRow, PgPool};
use std::net::IpAddr;
use uuid::Uuid;

pub struct PgApiKeyRepository {
    pub pool: PgPool,
}

pub struct PgAuditLoggerAdapter {
    inner: PgAuditLogger,
}

impl PgAuditLoggerAdapter {
    pub fn new(inner: PgAuditLogger) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl AuditLogger for PgAuditLoggerAdapter {
    async fn record(&self, entry: AuditEntry) -> Result<(), KeyServiceError> {
        self.inner
            .record(entry)
            .await
            .map_err(KeyServiceError::DependencyUnavailable)
    }
}
#[derive(FromRow)]
struct DbApiKey {
    id: Uuid,
    user_id: Uuid,
    name: String,
    key_prefix: String,
    key_hash: String,
    scopes: Vec<String>,
    allowed_ips: Option<Vec<String>>,
    rate_limit_override: Option<Value>,
    status: String,
    expires_at: Option<DateTime<Utc>>,
    last_used_at: Option<DateTime<Utc>>,
    last_used_ip: Option<String>,
    revoked_at: Option<DateTime<Utc>>,
    revoked_reason: Option<String>,
    created_at: DateTime<Utc>,
}
fn map_error(error: sqlx::Error) -> KeyServiceError {
    KeyServiceError::DependencyUnavailable(error.to_string())
}
fn map_key(row: DbApiKey) -> Result<ApiKey, KeyServiceError> {
    let scopes = row
        .scopes
        .iter()
        .map(|scope| ApiKeyScope::parse(scope))
        .collect::<Result<Vec<_>, _>>()?;
    let allowed_ips = row
        .allowed_ips
        .unwrap_or_default()
        .iter()
        .map(|ip| {
            ip.parse()
                .map_err(|_| KeyServiceError::Validation("invalid stored IP address".into()))
        })
        .collect::<Result<Vec<IpAddr>, _>>()?;
    let rate_limit_override = row
        .rate_limit_override
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| {
            KeyServiceError::Validation(format!("invalid stored rate limit override: {e}"))
        })?
        .unwrap_or(RateLimitOverride {
            rpm: None,
            tpm: None,
            concurrent: None,
        });
    Ok(ApiKey {
        id: row.id,
        user_id: row.user_id,
        name: row.name,
        key_prefix: row.key_prefix,
        key_hash: row.key_hash,
        scopes,
        allowed_ips,
        rate_limit_override,
        status: row.status,
        expires_at: row.expires_at,
        last_used_at: row.last_used_at,
        last_used_ip: row
            .last_used_ip
            .map(|ip| {
                ip.parse()
                    .map_err(|_| KeyServiceError::Validation("invalid stored last-used IP".into()))
            })
            .transpose()?,
        revoked_at: row.revoked_at,
        revoked_reason: row.revoked_reason,
        created_at: row.created_at,
    })
}
const SELECT: &str = "SELECT id,user_id,name,key_prefix,key_hash,scopes,allowed_ips::text[],rate_limit_override,status,expires_at,last_used_at,last_used_ip::text AS last_used_ip,revoked_at,revoked_reason,created_at FROM api_keys";
#[async_trait]
impl ApiKeyRepository for PgApiKeyRepository {
    async fn save(&self, key: &ApiKey) -> Result<(), KeyServiceError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1::text))")
            .bind(key.user_id)
            .execute(tx.as_mut())
            .await
            .map_err(map_error)?;
        sqlx::query("INSERT INTO api_keys (id,user_id,name,key_prefix,key_hash,scopes,allowed_ips,rate_limit_override,status,expires_at,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7::inet[],$8,$9,$10,$11)").bind(key.id).bind(key.user_id).bind(&key.name).bind(&key.key_prefix).bind(&key.key_hash).bind(key.scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>()).bind(key.allowed_ips.iter().map(ToString::to_string).collect::<Vec<_>>()).bind(serde_json::to_value(&key.rate_limit_override).map_err(|e| KeyServiceError::Validation(e.to_string()))?).bind(&key.status).bind(key.expires_at).bind(key.created_at).execute(tx.as_mut()).await.map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        Ok(())
    }

    async fn save_if_capacity(
        &self,
        key: &ApiKey,
        max_active_keys: i64,
    ) -> Result<bool, KeyServiceError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1::text))")
            .bind(key.user_id)
            .execute(tx.as_mut())
            .await
            .map_err(map_error)?;
        if max_active_keys > 0 {
            let active: i64 = sqlx::query_scalar(
                "SELECT count(*)::BIGINT FROM api_keys WHERE user_id=$1 AND status='active'",
            )
            .bind(key.user_id)
            .fetch_one(tx.as_mut())
            .await
            .map_err(map_error)?;
            if active >= max_active_keys {
                tx.rollback().await.map_err(map_error)?;
                return Ok(false);
            }
        }
        sqlx::query("INSERT INTO api_keys (id,user_id,name,key_prefix,key_hash,scopes,allowed_ips,rate_limit_override,status,expires_at,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7::inet[],$8,$9,$10,$11)")
            .bind(key.id)
            .bind(key.user_id)
            .bind(&key.name)
            .bind(&key.key_prefix)
            .bind(&key.key_hash)
            .bind(key.scopes.iter().map(|s| s.as_str()).collect::<Vec<_>>())
            .bind(key.allowed_ips.iter().map(ToString::to_string).collect::<Vec<_>>())
            .bind(serde_json::to_value(&key.rate_limit_override).map_err(|e| KeyServiceError::Validation(e.to_string()))?)
            .bind(&key.status)
            .bind(key.expires_at)
            .bind(key.created_at)
            .execute(tx.as_mut())
            .await
            .map_err(map_error)?;
        tx.commit().await.map_err(map_error)?;
        Ok(true)
    }
    async fn find_by_id(&self, id: Uuid) -> Result<Option<ApiKey>, KeyServiceError> {
        let row = sqlx::query_as::<_, DbApiKey>(&format!("{SELECT} WHERE id=$1"))
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_error)?;
        row.map(map_key).transpose()
    }
    async fn find_by_hash(&self, hash: &str) -> Result<Option<ApiKey>, KeyServiceError> {
        let row = sqlx::query_as::<_, DbApiKey>(&format!("{SELECT} WHERE key_hash=$1"))
            .bind(hash)
            .fetch_optional(&self.pool)
            .await
            .map_err(map_error)?;
        row.map(map_key).transpose()
    }
    async fn find_by_user(&self, user_id: Uuid) -> Result<Vec<ApiKey>, KeyServiceError> {
        sqlx::query_as::<_, DbApiKey>(&format!(
            "{SELECT} WHERE user_id=$1 ORDER BY created_at DESC"
        ))
        .bind(user_id)
        .fetch_all(&self.pool)
        .await
        .map_err(map_error)?
        .into_iter()
        .map(map_key)
        .collect()
    }
    async fn count_active_by_user(&self, user_id: Uuid) -> Result<i64, KeyServiceError> {
        sqlx::query_scalar!(
            "SELECT count(*) FROM api_keys WHERE user_id=$1 AND status='active'",
            user_id,
        )
        .fetch_one(&self.pool)
        .await
        .map_err(map_error)
        .map(|c| c.unwrap_or(0))
    }
    async fn revoke(
        &self,
        id: Uuid,
        revoked_by: Uuid,
        reason: &str,
    ) -> Result<(), KeyServiceError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        let key = sqlx::query_as::<_, DbApiKey>(&format!("{SELECT} WHERE id=$1 FOR UPDATE"))
            .bind(id)
            .fetch_optional(tx.as_mut())
            .await
            .map_err(map_error)?
            .ok_or(KeyServiceError::ApiKeyNotFound)?;
        let result = sqlx::query!("UPDATE api_keys SET status='revoked', revoked_at=now(), revoked_reason=$2 WHERE id=$1 AND status='active'",
            id,
            reason)
            .execute(tx.as_mut())
            .await
            .map_err(map_error)?;
        if result.rows_affected() != 1 {
            return Err(KeyServiceError::KeyAlreadyRevoked);
        }
        let envelope = EventEnvelope::new(
            EVENT_API_KEY_REVOKED,
            ApiKeyRevoked {
                api_key_id: id,
                user_id: key.user_id,
                revoked_by,
                reason: reason.to_string(),
            },
        );
        enqueue_in_transaction(&mut tx, "event_outbox", EVENT_API_KEY_REVOKED, &envelope)
            .await
            .map_err(|error| KeyServiceError::DependencyUnavailable(error.to_string()))?;
        tx.commit().await.map_err(map_error)?;
        Ok(())
    }
    async fn update_last_used(
        &self,
        id: Uuid,
        source_ip: Option<IpAddr>,
    ) -> Result<(), KeyServiceError> {
        sqlx::query("UPDATE api_keys SET last_used_at=now(), last_used_ip=$2::inet WHERE id=$1")
            .bind(id)
            .bind(source_ip.map(|ip| ip.to_string()))
            .execute(&self.pool)
            .await
            .map_err(map_error)?;
        Ok(())
    }
    async fn update(&self, key: &ApiKey) -> Result<(), KeyServiceError> {
        let mut tx = self.pool.begin().await.map_err(map_error)?;
        sqlx::query(
            "UPDATE api_keys SET name=$2,allowed_ips=$3::inet[],rate_limit_override=$4,scopes=$5 WHERE id=$1",
        )
        .bind(key.id)
        .bind(&key.name)
        .bind(
            key.allowed_ips
                .iter()
                .map(ToString::to_string)
                .collect::<Vec<_>>(),
        )
        .bind(
            serde_json::to_value(&key.rate_limit_override)
                .map_err(|e| KeyServiceError::Validation(e.to_string()))?,
        )
        .bind(key.scopes.iter().map(|scope| scope.as_str()).collect::<Vec<_>>())
        .execute(tx.as_mut())
        .await
        .map_err(map_error)?;
        let envelope = EventEnvelope::new(
            EVENT_API_KEY_AUTHORIZATION_CHANGED,
            ApiKeyAuthorizationChanged {
                api_key_id: key.id,
                user_id: key.user_id,
                changed_at: Utc::now(),
                reason: "scopes or key metadata changed".into(),
            },
        );
        enqueue_in_transaction(
            &mut tx,
            "event_outbox",
            EVENT_API_KEY_AUTHORIZATION_CHANGED,
            &envelope,
        )
        .await
        .map_err(|error| KeyServiceError::DependencyUnavailable(error.to_string()))?;
        tx.commit().await.map_err(map_error)?;
        Ok(())
    }
}
