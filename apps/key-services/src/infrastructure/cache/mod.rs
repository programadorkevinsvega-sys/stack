use crate::application::ports::{cache_key, ApiKeyCache};
use crate::domain::entities::ApiKey;
use crate::domain::errors::KeyServiceError;
use async_trait::async_trait;
use common::ReloadableRedisPool;
use deadpool_redis::redis::AsyncCommands;
use moka::sync::Cache;
use std::time::Duration;
use uuid::Uuid;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct CachedApiKey {
    key: ApiKey,
    authz_version: u64,
}

fn authz_version_key(user_id: Uuid) -> String {
    format!("keys:authz:version:{user_id}")
}

pub struct RedisApiKeyCache {
    redis: ReloadableRedisPool,
    l1: Cache<String, CachedApiKey>,
    l2_ttl_seconds: u64,
}

impl RedisApiKeyCache {
    pub fn new(redis: ReloadableRedisPool, l1_ttl_seconds: u64, l2_ttl_seconds: u64) -> Self {
        let l1_ttl = Duration::from_secs(l1_ttl_seconds);
        let l2_ttl = Duration::from_secs(l2_ttl_seconds);
        if l1_ttl > l2_ttl {
            tracing::warn!(
                l1_ttl = l1_ttl_seconds,
                l2_ttl = l2_ttl_seconds,
                "L1 TTL is greater than L2 TTL; capping L1 TTL to L2 TTL"
            );
        }
        Self {
            redis,
            l1: Cache::builder()
                .time_to_live(std::cmp::min(l1_ttl, l2_ttl))
                .max_capacity(100_000)
                .build(),
            l2_ttl_seconds,
        }
    }
}

#[async_trait]
impl ApiKeyCache for RedisApiKeyCache {
    async fn get(&self, hash: &str) -> Result<Option<ApiKey>, KeyServiceError> {
        let cached = self.l1.get(hash);
        let mut connection = self
            .redis
            .get()
            .await
            .map_err(|e| KeyServiceError::DependencyUnavailable(e.to_string()))?;
        let value: Option<String> = connection
            .get(cache_key(hash))
            .await
            .map_err(|e| KeyServiceError::DependencyUnavailable(e.to_string()))?;
        let entry = if let Some(json) = value {
            serde_json::from_str::<CachedApiKey>(&json)
                .or_else(|_| {
                    serde_json::from_str::<ApiKey>(&json).map(|key| CachedApiKey {
                        key,
                        authz_version: 0,
                    })
                })
                .map_err(|e| KeyServiceError::Validation(format!("invalid cached api key: {e}")))?
        } else if let Some(entry) = cached {
            entry
        } else {
            return Ok(None);
        };
        let current: Option<u64> = connection
            .get(authz_version_key(entry.key.user_id))
            .await
            .map_err(|e| KeyServiceError::DependencyUnavailable(e.to_string()))?;
        if entry.authz_version != current.unwrap_or(0) {
            self.l1.invalidate(hash);
            let _: () = connection
                .del(cache_key(hash))
                .await
                .map_err(|e| KeyServiceError::DependencyUnavailable(e.to_string()))?;
            return Ok(None);
        }
        self.l1.insert(hash.to_string(), entry.clone());
        Ok(Some(entry.key))
    }

    async fn set(&self, key: &ApiKey) -> Result<(), KeyServiceError> {
        let mut connection = self
            .redis
            .get()
            .await
            .map_err(|e| KeyServiceError::DependencyUnavailable(e.to_string()))?;
        let version: Option<u64> = connection
            .get(authz_version_key(key.user_id))
            .await
            .map_err(|e| KeyServiceError::DependencyUnavailable(e.to_string()))?;
        let cached = CachedApiKey {
            key: key.clone(),
            authz_version: version.unwrap_or(0),
        };
        let json = serde_json::to_string(&cached)
            .map_err(|e| KeyServiceError::Validation(e.to_string()))?;
        connection
            .set_ex::<_, _, ()>(cache_key(&key.key_hash), json, self.l2_ttl_seconds)
            .await
            .map_err(|e| KeyServiceError::DependencyUnavailable(e.to_string()))?;
        self.l1.insert(key.key_hash.clone(), cached);
        Ok(())
    }

    async fn invalidate(&self, hash: &str) -> Result<(), KeyServiceError> {
        self.l1.invalidate(hash);
        let mut connection = self
            .redis
            .get()
            .await
            .map_err(|e| KeyServiceError::DependencyUnavailable(e.to_string()))?;
        connection
            .del::<_, ()>(cache_key(hash))
            .await
            .map_err(|e| KeyServiceError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }

    async fn invalidate_user(&self, user_id: Uuid) -> Result<(), KeyServiceError> {
        let mut connection = self
            .redis
            .get()
            .await
            .map_err(|e| KeyServiceError::DependencyUnavailable(e.to_string()))?;
        let _: u64 = connection
            .incr(authz_version_key(user_id), 1u64)
            .await
            .map_err(|e| KeyServiceError::DependencyUnavailable(e.to_string()))?;
        // L1 entries are validated against the epoch on their next access;
        // no unbounded in-memory scan is required here.
        Ok(())
    }
}
