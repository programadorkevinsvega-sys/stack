use crate::{AppConfig, ServiceTlsConfig};
use deadpool_redis::{
    redis, Config as RedisConfig, Pool as RedisPool, PoolConfig as RedisPoolConfig, Runtime,
    Timeouts,
};
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use thiserror::Error;
use tokio::sync::RwLock;
use url::Url;

#[derive(Debug, Error)]
pub enum RedisStartupError {
    #[error("failed to create Redis pool: {0}")]
    CreatePool(#[from] deadpool_redis::CreatePoolError),
    #[error("failed to obtain a Redis connection: {0}")]
    Pool(#[from] deadpool_redis::PoolError),
    #[error("Redis PING failed: {0}")]
    Ping(#[from] redis::RedisError),
}

/// Redis pool whose underlying connection manager can be replaced atomically.
/// Existing checked-out connections finish normally; subsequent calls obtain
/// connections from the validated replacement pool.
#[derive(Clone)]
pub struct ReloadableRedisPool {
    inner: Arc<RwLock<RedisPool>>,
    pool_size: usize,
    timeout_ms: u64,
}

impl ReloadableRedisPool {
    pub async fn get(&self) -> Result<deadpool_redis::Connection, deadpool_redis::PoolError> {
        let pool = self.inner.read().await.clone();
        pool.get().await
    }

    pub async fn replace_url(
        &self,
        service: &str,
        redis_url: &str,
    ) -> Result<(), RedisStartupError> {
        let replacement =
            create_redis_pool(service, redis_url, self.pool_size, self.timeout_ms).await?;
        let previous = {
            let mut current = self.inner.write().await;
            std::mem::replace(&mut *current, replacement)
        };
        previous.close();
        tracing::info!(
            service,
            "Redis pool replaced after validated credential rotation"
        );
        Ok(())
    }
}

/// Removes credentials and query parameters before an endpoint is written to logs.
pub fn sanitize_endpoint(endpoint: &str) -> String {
    let trimmed = endpoint.trim();
    let Ok(mut parsed) = Url::parse(trimmed) else {
        return if trimmed.is_empty() {
            "<empty>".to_string()
        } else {
            "<configured>".to_string()
        };
    };

    let _ = parsed.set_username("");
    let _ = parsed.set_password(None);
    parsed.set_query(None);
    parsed.set_fragment(None);
    parsed.to_string()
}

pub fn log_service_starting(service: &str, config: &AppConfig, tls_config: &ServiceTlsConfig) {
    let secrets_provider = std::env::var("SECRETS_PROVIDER").unwrap_or_else(|_| "env".to_string());
    let otel_endpoint = config
        .otel_exporter_otlp_endpoint
        .as_deref()
        .map(sanitize_endpoint);

    tracing::info!(
        service,
        secrets_provider = %secrets_provider,
        tls_mode = %tls_config.mode,
        tls_provider = %tls_config.provider,
        otel_enabled = config.otel_enabled.unwrap_or(false),
        otel_configured = otel_endpoint.is_some(),
        otel_endpoint = otel_endpoint.as_deref().unwrap_or("<disabled>"),
        "service startup configuration loaded"
    );
}

pub async fn connect_postgres(
    service: &str,
    database_url: &str,
    max_connections: u32,
    acquire_timeout_ms: u64,
    idle_timeout_ms: u64,
) -> Result<PgPool, sqlx::Error> {
    let endpoint = sanitize_endpoint(database_url);
    tracing::info!(
        service,
        dependency = "postgres",
        %endpoint,
        max_connections,
        acquire_timeout_ms,
        idle_timeout_ms,
        "connecting to dependency"
    );

    let pool = match PgPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(Duration::from_millis(acquire_timeout_ms))
        .idle_timeout(Duration::from_millis(idle_timeout_ms))
        .connect(database_url)
        .await
    {
        Ok(pool) => pool,
        Err(error) => {
            tracing::error!(
                service,
                dependency = "postgres",
                %endpoint,
                error = %error,
                "dependency connection failed"
            );
            return Err(error);
        }
    };

    let metadata = sqlx::query_as::<_, (String, String)>(
        "SELECT current_database(), current_setting('server_version')",
    )
    .fetch_one(&pool)
    .await;
    match metadata {
        Ok((database, server_version)) => {
            tracing::info!(
                service,
                dependency = "postgres",
                %endpoint,
                %database,
                %server_version,
                max_connections,
                "dependency connected"
            );
        }
        Err(error) => {
            tracing::error!(
                service,
                dependency = "postgres",
                %endpoint,
                error = %error,
                "dependency health check failed"
            );
            pool.close().await;
            return Err(error);
        }
    }

    Ok(pool)
}

pub async fn connect_redis(
    service: &str,
    redis_url: &str,
    pool_size: usize,
    timeout_ms: u64,
) -> Result<ReloadableRedisPool, RedisStartupError> {
    let endpoint = sanitize_endpoint(redis_url);
    tracing::info!(
        service,
        dependency = "redis",
        %endpoint,
        pool_size,
        timeout_ms,
        "connecting to dependency"
    );

    let pool = create_redis_pool(service, redis_url, pool_size, timeout_ms).await?;
    Ok(ReloadableRedisPool {
        inner: Arc::new(RwLock::new(pool)),
        pool_size,
        timeout_ms,
    })
}

async fn create_redis_pool(
    service: &str,
    redis_url: &str,
    pool_size: usize,
    timeout_ms: u64,
) -> Result<RedisPool, RedisStartupError> {
    let endpoint = sanitize_endpoint(redis_url);
    let mut redis_config = RedisConfig::from_url(redis_url.to_string());
    let mut pool_config = RedisPoolConfig::new(pool_size);
    pool_config.timeouts = Timeouts::wait_millis(timeout_ms);
    redis_config.pool = Some(pool_config);
    let pool = match redis_config.create_pool(Some(Runtime::Tokio1)) {
        Ok(pool) => pool,
        Err(error) => {
            tracing::error!(
                service,
                dependency = "redis",
                %endpoint,
                error = %error,
                "dependency pool creation failed"
            );
            return Err(error.into());
        }
    };

    let mut connection = match pool.get().await {
        Ok(connection) => connection,
        Err(error) => {
            tracing::error!(
                service,
                dependency = "redis",
                %endpoint,
                error = %error,
                "dependency connection failed"
            );
            return Err(error.into());
        }
    };
    let pong = redis::cmd("PING")
        .query_async::<String>(&mut connection)
        .await;
    match pong {
        Ok(response) if response == "PONG" => {
            tracing::info!(
                service,
                dependency = "redis",
                %endpoint,
                pool_size,
                health_check = "PING",
                "dependency connected"
            );
        }
        Ok(response) => {
            let error = redis::RedisError::from((
                redis::ErrorKind::UnexpectedReturnType,
                "unexpected Redis PING response",
                response,
            ));
            tracing::error!(
                service,
                dependency = "redis",
                %endpoint,
                error = %error,
                "dependency health check failed"
            );
            return Err(error.into());
        }
        Err(error) => {
            tracing::error!(
                service,
                dependency = "redis",
                %endpoint,
                error = %error,
                "dependency health check failed"
            );
            return Err(error.into());
        }
    }

    drop(connection);
    Ok(pool)
}

#[cfg(test)]
mod tests {
    use super::sanitize_endpoint;

    #[test]
    fn strips_credentials_and_query_parameters() {
        assert_eq!(
            sanitize_endpoint(
                "postgres://user:password@localhost:5432/catalog?sslmode=disable#fragment"
            ),
            "postgres://localhost:5432/catalog"
        );
    }

    #[test]
    fn does_not_echo_unparseable_values() {
        assert_eq!(sanitize_endpoint("secret endpoint value"), "<configured>");
        assert_eq!(sanitize_endpoint(""), "<empty>");
    }
}
