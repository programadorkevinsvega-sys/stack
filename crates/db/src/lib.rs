use serde::{Deserialize, Serialize};
use std::time::Instant;

// ── Type Aliases ──────────────────────────────────────────────────────

pub type PgPool = sqlx::PgPool;
pub type RedisPool = common::ReloadableRedisPool;

// ── Database config ───────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct DbConfig {
    pub max_connections: u32,
    pub acquire_timeout_ms: u64,
    pub idle_timeout_ms: u64,
}

impl Default for DbConfig {
    fn default() -> Self {
        Self {
            max_connections: 20,
            acquire_timeout_ms: 5000,
            idle_timeout_ms: 300_000,
        }
    }
}

impl DbConfig {
    pub fn from_env() -> Result<Self, envy::Error> {
        envy::prefixed("DATABASE_").from_env::<Self>()
    }
}

// ── Redis config ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    pub pool_size: usize,
    pub timeout_ms: u64,
}

impl Default for RedisConfig {
    fn default() -> Self {
        Self {
            pool_size: 16,
            timeout_ms: 2000,
        }
    }
}

impl RedisConfig {
    pub fn from_env() -> Result<Self, envy::Error> {
        envy::prefixed("REDIS_").from_env::<Self>()
    }
}

// ── Health check ──────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct HealthCheckResult {
    pub postgres_ok: bool,
    pub redis_ok: bool,
    pub nats_ok: bool,
    pub latency_ms: u64,
}

pub async fn check_health(pg: &PgPool, redis: &RedisPool, nats_url: &str) -> HealthCheckResult {
    let start = Instant::now();

    let pg_fut = check_pg(pg);
    let redis_fut = check_redis(redis);
    let nats_fut = check_nats(nats_url);

    let (postgres_ok, redis_ok, nats_ok) = tokio::join!(pg_fut, redis_fut, nats_fut);

    HealthCheckResult {
        postgres_ok,
        redis_ok,
        nats_ok,
        latency_ms: start.elapsed().as_millis() as u64,
    }
}

async fn check_pg(pg: &PgPool) -> bool {
    sqlx::query_scalar::<_, i32>("SELECT 1")
        .fetch_one(pg)
        .await
        .is_ok()
}

async fn check_redis(redis: &RedisPool) -> bool {
    let mut conn = match redis.get().await {
        Ok(c) => c,
        Err(_) => return false,
    };
    deadpool_redis::redis::cmd("PING")
        .query_async::<String>(&mut *conn)
        .await
        .map(|reply| reply == "PONG")
        .unwrap_or(false)
}

async fn check_nats(nats_url: &str) -> bool {
    if nats_url.is_empty() {
        return true;
    }
    let client = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        common::connect_nats(nats_url),
    )
    .await;
    match client {
        Ok(Ok(connection)) => {
            drop(connection);
            true
        }
        Ok(Err(error)) => {
            tracing::debug!(error = %error, "database connectivity probe failed");
            false
        }
        Err(error) => {
            tracing::debug!(error = %error, "database connectivity probe timed out");
            false
        }
    }
}
