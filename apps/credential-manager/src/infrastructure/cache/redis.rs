use crate::application::ports::StsSessionCache;
use crate::domain::entities::StsSessionToken;
use async_trait::async_trait;
use common::ReloadableRedisPool;
use deadpool_redis::redis::AsyncCommands;
use uuid::Uuid;

pub struct RedisStsSessionCache {
    client: ReloadableRedisPool,
    ttl_seconds: i64,
    refresh_buffer_seconds: i64,
}

impl RedisStsSessionCache {
    pub fn new(
        client: ReloadableRedisPool,
        sts_session_duration_seconds: i32,
        refresh_buffer_seconds: i64,
    ) -> Self {
        Self {
            client,
            ttl_seconds: sts_session_duration_seconds as i64,
            refresh_buffer_seconds,
        }
    }

    fn redis_key(account_id: Uuid) -> String {
        format!("sts:session:{account_id}")
    }
}

#[async_trait]
impl StsSessionCache for RedisStsSessionCache {
    async fn get_valid(&self, account_id: Uuid) -> Option<StsSessionToken> {
        let key = Self::redis_key(account_id);
        let mut conn = self.client.get().await.ok()?;
        let data: Option<String> = conn.get(&key).await.ok()?;
        match data {
            Some(json) => {
                let token: StsSessionToken = serde_json::from_str(&json).ok()?;
                token.lock_memory();
                if token.is_expiring_within(self.refresh_buffer_seconds) {
                    None
                } else {
                    Some(token)
                }
            }
            None => None,
        }
    }

    async fn set(&self, token: StsSessionToken) {
        let key = Self::redis_key(token.account_id);
        let json = match serde_json::to_string(&token) {
            Ok(j) => j,
            Err(e) => {
                tracing::warn!(error = %e, account_id = %token.account_id, "Failed to serialize STS token for Redis cache");
                return;
            }
        };
        let mut conn = match self.client.get().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, account_id = %token.account_id, "Failed to get Redis connection for STS token cache");
                return;
            }
        };
        if let Err(e) = conn
            .set_ex::<_, _, ()>(&key, &json, self.ttl_seconds as u64)
            .await
        {
            tracing::warn!(error = %e, account_id = %token.account_id, "Failed to store STS token in Redis cache");
        }
    }

    async fn invalidate(&self, account_id: Uuid) {
        let key = Self::redis_key(account_id);
        let mut conn = match self.client.get().await {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!(error = %e, account_id = %account_id, "Failed to get Redis connection for STS token invalidation");
                return;
            }
        };
        if let Err(e) = conn.del::<_, ()>(&key).await {
            tracing::warn!(error = %e, account_id = %account_id, "Failed to delete STS token from Redis cache");
        }
    }
}
