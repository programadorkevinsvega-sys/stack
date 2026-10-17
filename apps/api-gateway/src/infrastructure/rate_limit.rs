use crate::domain::errors::GatewayError;
use common::ReloadableRedisPool;
use deadpool_redis::redis::AsyncCommands;
use uuid::Uuid;

#[derive(Clone)]
pub struct RedisRateLimiter {
    pub pool: ReloadableRedisPool,
}

pub struct RateLimitInput {
    pub api_key_id: Uuid,
    pub rpm_limit: i32,
    pub tpm_limit: i32,
    pub concurrent_limit: i32,
    pub estimated_tokens: i64,
}

impl RedisRateLimiter {
    pub async fn check(&self, input: RateLimitInput) -> Result<(), GatewayError> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| GatewayError::UpstreamUnavailable(e.to_string()))?;
        let minute = chrono::Utc::now().timestamp() / 60;
        let rpm_key = format!("api_gateway:rpm:{}:{minute}", input.api_key_id);
        let tpm_key = format!("api_gateway:tpm:{}:{minute}", input.api_key_id);
        let concurrent_key = format!("api_gateway:concurrent:{}", input.api_key_id);
        let concurrent: i64 = conn
            .incr(concurrent_key.clone(), 1)
            .await
            .map_err(redis_err)?;
        let _: bool = conn
            .expire(concurrent_key.clone(), 300)
            .await
            .map_err(redis_err)?;
        if concurrent > i64::from(input.concurrent_limit) {
            let _: i64 = conn.decr(concurrent_key, 1).await.map_err(redis_err)?;
            return Err(GatewayError::RateLimited);
        }
        let rpm: i64 = conn.incr(rpm_key.clone(), 1).await.map_err(redis_err)?;
        let _: bool = conn.expire(rpm_key, 120).await.map_err(redis_err)?;
        let tpm: i64 = conn
            .incr(tpm_key.clone(), input.estimated_tokens.max(0))
            .await
            .map_err(redis_err)?;
        let _: bool = conn.expire(tpm_key, 120).await.map_err(redis_err)?;
        if rpm > i64::from(input.rpm_limit) || tpm > i64::from(input.tpm_limit) {
            self.finish(input.api_key_id).await;
            return Err(GatewayError::RateLimited);
        }
        Ok(())
    }

    pub async fn finish(&self, api_key_id: Uuid) {
        if let Ok(mut conn) = self.pool.get().await {
            let key = format!("api_gateway:concurrent:{api_key_id}");
            let active: Result<i64, _> = conn.decr(key.clone(), 1).await;
            if matches!(active, Ok(value) if value < 0) {
                let _: Result<(), _> = conn.set(key, 0).await;
            }
        }
    }
}

fn redis_err(error: deadpool_redis::redis::RedisError) -> GatewayError {
    GatewayError::UpstreamUnavailable(format!("redis error: {error}"))
}
