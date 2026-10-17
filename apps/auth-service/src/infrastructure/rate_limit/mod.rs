use crate::application::ports::LoginRateLimiter;
use crate::domain::errors::AuthError;
use async_trait::async_trait;
use common::ReloadableRedisPool;
use deadpool_redis::redis::AsyncCommands;
use sha2::{Digest, Sha256};

pub struct RedisLoginRateLimiter {
    pool: ReloadableRedisPool,
    max_attempts: u32,
    window_seconds: u32,
}

impl RedisLoginRateLimiter {
    pub fn new(
        pool: ReloadableRedisPool,
        max_attempts: u32,
        window_seconds: u32,
    ) -> Result<Self, AuthError> {
        if max_attempts == 0 {
            return Err(AuthError::Validation(
                "AUTH_LOGIN_RATE_LIMIT_MAX_ATTEMPTS must be > 0".into(),
            ));
        }
        if window_seconds == 0 {
            return Err(AuthError::Validation(
                "AUTH_LOGIN_RATE_LIMIT_WINDOW_SECONDS must be > 0".into(),
            ));
        }

        Ok(Self {
            pool,
            max_attempts,
            window_seconds,
        })
    }

    fn key(&self, email: &str, ip: &str) -> String {
        let hash = hex::encode(Sha256::digest(format!("{email}|{ip}").as_bytes()));
        format!("auth:login:attempts:{hash}")
    }

    fn email_key(&self, email: &str) -> String {
        let hash = hex::encode(Sha256::digest(email.as_bytes()));
        format!("auth:login:attempts:email:{hash}")
    }

    async fn check_allow(&self, key: &str, label: &str) -> Result<(), AuthError> {
        let mut conn = self.pool.get().await.map_err(|e| {
            AuthError::DependencyUnavailable(format!(
                "redis pool unavailable for auth rate-limit: {e}"
            ))
        })?;
        let current_attempts: Option<u32> = conn.get(key).await.map_err(|e| {
            AuthError::DependencyUnavailable(format!("redis GET failed for auth rate-limit: {e}"))
        })?;
        let attempts = current_attempts.unwrap_or(0);
        if attempts >= self.max_attempts {
            let ttl: i64 = conn.ttl(key).await.map_err(|e| {
                AuthError::DependencyUnavailable(format!(
                    "redis TTL failed for auth rate-limit: {e}"
                ))
            })?;
            let retry_after_seconds = if ttl > 0 {
                ttl as u32
            } else {
                self.window_seconds
            };
            return Err(AuthError::RateLimited(format!(
                "too many {label} login attempts, retry in {retry_after_seconds} seconds"
            )));
        }
        Ok(())
    }

    async fn register(&self, key: &str) -> Result<(), AuthError> {
        let mut conn = self.pool.get().await.map_err(|e| {
            AuthError::DependencyUnavailable(format!(
                "redis pool unavailable for auth rate-limit: {e}"
            ))
        })?;
        let count: u64 = conn.incr(key, 1).await.map_err(|e| {
            AuthError::DependencyUnavailable(format!("redis INCR failed for auth rate-limit: {e}"))
        })?;
        if count == 1 {
            let _: bool = conn
                .expire(key, self.window_seconds as i64)
                .await
                .map_err(|e| {
                    AuthError::DependencyUnavailable(format!(
                        "redis EXPIRE failed for auth rate-limit: {e}"
                    ))
                })?;
        }
        Ok(())
    }
}

#[async_trait]
impl LoginRateLimiter for RedisLoginRateLimiter {
    async fn ensure_allowed(&self, email: &str, ip: &str) -> Result<(), AuthError> {
        self.check_allow(&self.key(email, ip), "IP").await?;
        self.check_allow(&self.email_key(email), "email").await
    }

    async fn register_failure(&self, email: &str, ip: &str) -> Result<(), AuthError> {
        self.register(&self.key(email, ip)).await?;
        self.register(&self.email_key(email)).await
    }

    async fn reset(&self, email: &str, ip: &str) -> Result<(), AuthError> {
        let mut conn = self.pool.get().await.map_err(|e| {
            AuthError::DependencyUnavailable(format!(
                "redis pool unavailable for auth rate-limit: {e}"
            ))
        })?;
        let _: i64 = conn.del(self.key(email, ip)).await.map_err(|e| {
            AuthError::DependencyUnavailable(format!("redis DEL failed for auth rate-limit: {e}"))
        })?;
        let _: i64 = conn.del(self.email_key(email)).await.map_err(|e| {
            AuthError::DependencyUnavailable(format!("redis DEL failed for auth rate-limit: {e}"))
        })?;
        Ok(())
    }
}
