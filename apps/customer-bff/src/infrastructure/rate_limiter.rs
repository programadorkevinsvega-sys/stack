use crate::domain::errors::BffError;
use common::ReloadableRedisPool;
use deadpool_redis::redis::AsyncCommands;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct RateLimitInfo {
    pub remaining: usize,
    pub reset_at_secs: i64,
}

#[derive(Clone)]
pub struct RedisRateLimiter {
    pool: ReloadableRedisPool,
    ip_max_requests: usize,
    user_max_requests: usize,
}

impl RedisRateLimiter {
    pub fn new(
        pool: ReloadableRedisPool,
        ip_max_requests: usize,
        user_max_requests: usize,
    ) -> Self {
        Self {
            pool,
            ip_max_requests,
            user_max_requests,
        }
    }

    fn minute_window() -> (i64, i64) {
        let now = chrono::Utc::now();
        let minute = now.timestamp() / 60;
        let reset_at_secs = (minute + 1) * 60;
        (minute, reset_at_secs)
    }

    pub async fn check_ip(&self, ip: &str) -> Result<(RateLimitInfo, Option<BffError>), BffError> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| BffError::UpstreamUnavailable(format!("redis: {e}")))?;
        let (minute, reset_at_secs) = Self::minute_window();
        let key = format!("customer_bff:ip_rate:{ip}:{minute}");
        let count: u64 = conn
            .incr(&key, 1)
            .await
            .map_err(|e| BffError::UpstreamUnavailable(format!("redis: {e}")))?;
        if count == 1 {
            let _: Result<(), _> = conn.expire(&key, 120).await;
        }
        let max = self.ip_max_requests as u64;
        let remaining = max.saturating_sub(count) as usize;
        let error = if count > max {
            Some(BffError::RateLimited)
        } else {
            None
        };
        Ok((
            RateLimitInfo {
                remaining,
                reset_at_secs,
            },
            error,
        ))
    }

    pub async fn check_user(
        &self,
        user_id: Uuid,
    ) -> Result<(RateLimitInfo, Option<BffError>), BffError> {
        let mut conn = self
            .pool
            .get()
            .await
            .map_err(|e| BffError::UpstreamUnavailable(format!("redis: {e}")))?;
        let (minute, reset_at_secs) = Self::minute_window();
        let key = format!("customer_bff:user_rate:{user_id}:{minute}");
        let count: u64 = conn
            .incr(&key, 1)
            .await
            .map_err(|e| BffError::UpstreamUnavailable(format!("redis: {e}")))?;
        if count == 1 {
            let _: Result<(), _> = conn.expire(&key, 120).await;
        }
        let max = self.user_max_requests as u64;
        let remaining = max.saturating_sub(count) as usize;
        let error = if count > max {
            Some(BffError::RateLimited)
        } else {
            None
        };
        Ok((
            RateLimitInfo {
                remaining,
                reset_at_secs,
            },
            error,
        ))
    }
}
