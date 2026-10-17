use crate::application::ports::KeyCreationRateLimiter;
use crate::domain::errors::KeyServiceError;
use common::ReloadableRedisPool;
use uuid::Uuid;

pub struct RedisKeyCreationRateLimiter {
    pool: ReloadableRedisPool,
    max_creations: u64,
    window_seconds: u64,
}

async fn check_counter(
    pool: &ReloadableRedisPool,
    key: String,
    max: u64,
    window_seconds: u64,
) -> Result<(), KeyServiceError> {
    let mut conn = pool
        .get()
        .await
        .map_err(|e| KeyServiceError::DependencyUnavailable(e.to_string()))?;
    let lua = r#"
        local count = redis.call('INCR', KEYS[1])
        if count == 1 then redis.call('EXPIRE', KEYS[1], ARGV[1]) end
        return count
    "#;
    let count: u64 = deadpool_redis::redis::cmd("EVAL")
        .arg(lua)
        .arg(1usize)
        .arg(&key)
        .arg(window_seconds)
        .query_async(&mut *conn)
        .await
        .map_err(|e| KeyServiceError::DependencyUnavailable(e.to_string()))?;
    if count > max {
        return Err(KeyServiceError::RateLimit(format!(
            "max {max} operations per {window_seconds} seconds"
        )));
    }
    Ok(())
}

impl RedisKeyCreationRateLimiter {
    pub fn new(pool: ReloadableRedisPool, max_creations: u64, window_seconds: u64) -> Self {
        Self {
            pool,
            max_creations,
            window_seconds,
        }
    }
}

#[async_trait::async_trait]
impl KeyCreationRateLimiter for RedisKeyCreationRateLimiter {
    async fn check(&self, user_id: Uuid) -> Result<(), KeyServiceError> {
        let key = format!("keys:ratelimit:create:{user_id}");
        check_counter(&self.pool, key, self.max_creations, self.window_seconds).await
    }

    async fn check_rotation(&self, user_id: Uuid) -> Result<(), KeyServiceError> {
        let max = std::env::var("KEY_SERVICES_ROTATE_KEY_RATE_LIMIT_MAX")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(self.max_creations);
        let window = std::env::var("KEY_SERVICES_ROTATE_KEY_RATE_LIMIT_WINDOW_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(self.window_seconds);
        check_counter(
            &self.pool,
            format!("keys:ratelimit:rotate:{user_id}"),
            max,
            window,
        )
        .await
    }
}
