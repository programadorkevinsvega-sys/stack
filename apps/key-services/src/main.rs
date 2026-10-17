use anyhow::{Context, Result};
use common::{AppConfig, GrpcRateLimiter, ServiceJwtAuth, ServiceTlsConfig};
use key_services::application::commands::{
    CreateApiKey, RevokeApiKey, RotateApiKey, UpdateKeyMetadata,
};
use key_services::application::ports::{
    ApiKeyCache, AuditLogger, KeyCreationRateLimiter, KeyGenerator,
};
use key_services::application::queries::{GetApiKey, ListApiKeys, ValidateKey};
use key_services::infrastructure::cache::RedisApiKeyCache;
use key_services::infrastructure::crypto::HmacApiKeyGenerator;
use key_services::infrastructure::persistence::{PgApiKeyRepository, PgAuditLoggerAdapter};
use key_services::infrastructure::plan::SubscriptionPlanLimitsClient;
use key_services::infrastructure::rate_limit::RedisKeyCreationRateLimiter;
use key_services::interfaces::grpc::{KeyGrpcService, KeyManagementGrpcService};
use proto::key_validation_v1::key_management_service_server::KeyManagementServiceServer;
use proto::key_validation_v1::key_validation_service_server::KeyValidationServiceServer;
use std::collections::HashSet;
use std::sync::Arc;
use tonic::transport::Server;
use tonic::Request;
use tracing::info;

fn required<T>(value: Option<T>, name: &str) -> Result<T> {
    value.with_context(|| format!("{name} is required"))
}

fn csv_env(name: &str) -> Result<HashSet<String>> {
    let values = std::env::var(name)
        .with_context(|| format!("{name} is required"))?
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<HashSet<_>>();
    if values.is_empty() {
        anyhow::bail!("{name} cannot be empty");
    }
    Ok(values)
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env_async()
        .await
        .context("failed to load configuration")?;
    let config_reload = common::ConfigReloadHandle::start(config.clone());
    common::observability::init("key-services", &config)
        .context("failed to initialize observability")?;
    let service_tls = ServiceTlsConfig::from_env("key-services")
        .context("invalid key-services TLS configuration")?;
    common::startup::log_service_starting("key-services", &config, &service_tls);
    let max_connections = required(config.database_max_connections, "DATABASE_MAX_CONNECTIONS")?;
    let acquire_timeout = required(
        config.database_acquire_timeout_ms,
        "DATABASE_ACQUIRE_TIMEOUT_MS",
    )?;
    let idle_timeout = required(config.database_idle_timeout_ms, "DATABASE_IDLE_TIMEOUT_MS")?;
    let redis_pool_size = required(config.redis_pool_size, "REDIS_POOL_SIZE")?;
    let redis_timeout = required(config.redis_timeout_ms, "REDIS_TIMEOUT_MS")?;
    let pepper = required(config.api_key_pepper, "API_KEY_PEPPER")?;
    let mut secrets: Vec<String> = vec![required(
        config.service_jwt_secret.clone(),
        "SERVICE_JWT_SECRET",
    )?];
    if let Ok(prev) = std::env::var("SERVICE_JWT_PREVIOUS_SECRET") {
        if !prev.is_empty() && prev != secrets[0] {
            secrets.push(prev);
        }
    }
    let mut allowed_issuers = csv_env("KEY_SERVICES_ALLOWED_ISSUERS")?;
    let mut allowed_subjects = csv_env("KEY_SERVICES_ALLOWED_SUBJECTS")?;
    // The S3-compatible gateway is a first-class internal caller. Keep it in
    // the allowlist even when older .env files predate the service.
    allowed_issuers.insert("s3-gateway".into());
    allowed_subjects.insert("s3-gateway".into());
    let service_auth = Arc::new(
        ServiceJwtAuth::with_secrets(&secrets, allowed_issuers, allowed_subjects)
            .map_err(|e| anyhow::anyhow!("{e}"))?,
    );
    let bind_addr =
        std::env::var("KEY_SERVICES_BIND_ADDR").context("KEY_SERVICES_BIND_ADDR is required")?;
    let l1_ttl = std::env::var("KEY_SERVICES_L1_CACHE_TTL_SECONDS")
        .context("KEY_SERVICES_L1_CACHE_TTL_SECONDS is required")?
        .parse::<u64>()
        .context("invalid KEY_SERVICES_L1_CACHE_TTL_SECONDS")?;
    let l2_ttl = std::env::var("KEY_SERVICES_L2_CACHE_TTL_SECONDS")
        .context("KEY_SERVICES_L2_CACHE_TTL_SECONDS is required")?
        .parse::<u64>()
        .context("invalid KEY_SERVICES_L2_CACHE_TTL_SECONDS")?;
    let db_url = std::env::var("KEY_SERVICES_DATABASE_URL")
        .context("KEY_SERVICES_DATABASE_URL is required")?;
    let pool = common::startup::connect_postgres(
        "key-services",
        &db_url,
        max_connections,
        acquire_timeout,
        idle_timeout,
    )
    .await
    .context("failed to connect postgres")?;
    config_reload.register_postgres_pool("key-services", "KEY_SERVICES_DATABASE_URL", pool.clone());
    let redis = common::startup::connect_redis(
        "key-services",
        &config.redis_url,
        redis_pool_size,
        redis_timeout,
    )
    .await
    .context("failed to create redis pool")?;
    config_reload.register_redis_pool("key-services", "REDIS_URL", redis.clone());
    let cache: Arc<dyn ApiKeyCache> =
        Arc::new(RedisApiKeyCache::new(redis.clone(), l1_ttl, l2_ttl));
    let max_creations: u64 = std::env::var("KEY_SERVICES_CREATE_KEY_RATE_LIMIT_MAX")
        .unwrap_or_else(|_| "5".to_string())
        .parse()
        .context("KEY_SERVICES_CREATE_KEY_RATE_LIMIT_MAX must be a valid u64")?;
    let rate_window: u64 = std::env::var("KEY_SERVICES_CREATE_KEY_RATE_LIMIT_WINDOW_SECONDS")
        .unwrap_or_else(|_| "3600".to_string())
        .parse()
        .context("KEY_SERVICES_CREATE_KEY_RATE_LIMIT_WINDOW_SECONDS must be a valid u64")?;
    let grpc_rate_limiter = Arc::new(GrpcRateLimiter::new(
        redis.clone(),
        std::env::var("GRPC_RATE_LIMIT_RPM")
            .unwrap_or_else(|_| "1000".to_string())
            .parse::<u64>()
            .context("GRPC_RATE_LIMIT_RPM must be a valid u64")?,
    ));
    let key_creation_rate_limiter: Arc<dyn KeyCreationRateLimiter> = Arc::new(
        RedisKeyCreationRateLimiter::new(redis, max_creations, rate_window),
    );
    let generator: Arc<dyn KeyGenerator> = Arc::new(HmacApiKeyGenerator::new(pepper)?);
    let plan_limits: Arc<dyn key_services::application::ports::ApiKeyPlanLimits> =
        Arc::new(SubscriptionPlanLimitsClient {
            endpoint: std::env::var("KEY_SERVICES_SUBSCRIPTION_SERVICE_GRPC_ENDPOINT")
                .or_else(|_| std::env::var("SUBSCRIPTION_SERVICE_GRPC_ENDPOINT"))
                .unwrap_or_else(|_| "https://127.0.0.1:50065".into()),
            secret: config
                .service_jwt_secret
                .clone()
                .context("SERVICE_JWT_SECRET required")?,
            issuer: std::env::var("KEY_SERVICES_SERVICE_JWT_ISSUER")
                .unwrap_or_else(|_| "key-services".into()),
            subject: std::env::var("KEY_SERVICES_SERVICE_JWT_SUBJECT")
                .unwrap_or_else(|_| "key-services".into()),
            tls: service_tls.clone(),
        });
    let nats = common::connect_nats(&config.nats_url)
        .await
        .context("failed to connect nats")?;
    config_reload.register_nats_client("key-services", "NATS_URL", nats.clone());
    let jetstream = async_nats::jetstream::new(nats);
    events::jetstream::ensure_streams(&jetstream)
        .await
        .context("failed to configure JetStream streams")?;
    let relay = events::outbox::OutboxRelay::new(
        pool.clone(),
        jetstream,
        events::outbox::OutboxConfig::for_service("key-services", "event_outbox"),
    )
    .context("failed to configure key-services outbox relay")?;
    tokio::spawn(relay.run());

    let audit_hmac_secret = std::env::var("AUDIT_HMAC_SECRET")
        .or_else(|_| std::env::var("SERVICE_JWT_SECRET"))
        .context("AUDIT_HMAC_SECRET or SERVICE_JWT_SECRET is required")?;
    let audit_logger: Arc<dyn AuditLogger> =
        Arc::new(PgAuditLoggerAdapter::new(common::PgAuditLogger {
            pool: pool.clone(),
            signer: common::AuditSigner::new(audit_hmac_secret),
        }));

    let repo = Arc::new(PgApiKeyRepository { pool });
    let grpc = KeyGrpcService {
        validate_key: ValidateKey {
            repo: repo.clone(),
            cache: cache.clone(),
            generator: generator.clone(),
        },
        cache,
    };
    let revoke = RevokeApiKey {
        repo: repo.clone(),
        cache: grpc.cache.clone(),
        audit_logger: audit_logger.clone(),
    };
    let management = KeyManagementGrpcService {
        create: CreateApiKey {
            repo: repo.clone(),
            generator: generator.clone(),
            rate_limiter: key_creation_rate_limiter.clone(),
            audit_logger: audit_logger.clone(),
            plan_limits: plan_limits.clone(),
        },
        rotate: RotateApiKey {
            create: CreateApiKey {
                repo: repo.clone(),
                generator: generator.clone(),
                rate_limiter: key_creation_rate_limiter,
                audit_logger: audit_logger.clone(),
                plan_limits,
            },
            revoke: RevokeApiKey {
                repo: revoke.repo.clone(),
                cache: revoke.cache.clone(),
                audit_logger: audit_logger.clone(),
            },
        },
        update: UpdateKeyMetadata {
            repo: repo.clone(),
            cache: grpc.cache.clone(),
            audit_logger: audit_logger.clone(),
        },
        list: ListApiKeys { repo: repo.clone() },
        get: GetApiKey { repo },
        revoke,
    };
    let port = required(config.grpc_port, "GRPC_PORT")?;
    let address: std::net::SocketAddr = format!("{bind_addr}:{port}")
        .parse()
        .context("invalid key-services bind address")?;
    info!(%address, "key-services gRPC listening");
    let validation_auth = Arc::clone(&service_auth);
    let validation_rate_limiter = grpc_rate_limiter.clone();
    let validation_interceptor = move |mut request: Request<()>| {
        let token = request
            .metadata()
            .get("authorization")
            .ok_or_else(|| tonic::Status::unauthenticated("missing authorization header"))?
            .to_str()
            .map_err(|_| tonic::Status::unauthenticated("invalid authorization header"))?;
        let token = token
            .strip_prefix("Bearer ")
            .or_else(|| token.strip_prefix("bearer "))
            .ok_or_else(|| tonic::Status::unauthenticated("bearer token required"))?;
        let claims = validation_auth.verify(token)?;
        let rate_limiter = validation_rate_limiter.clone();
        let caller = claims.sub.clone();
        tokio::spawn(async move {
            let _ = rate_limiter.check(&caller).await;
        });
        request.extensions_mut().insert(claims);
        Ok(request)
    };
    let management_auth = Arc::clone(&service_auth);
    let management_rate_limiter = grpc_rate_limiter.clone();
    let management_interceptor = move |mut request: Request<()>| {
        let token = request
            .metadata()
            .get("authorization")
            .ok_or_else(|| tonic::Status::unauthenticated("missing authorization header"))?
            .to_str()
            .map_err(|_| tonic::Status::unauthenticated("invalid authorization header"))?;
        let token = token
            .strip_prefix("Bearer ")
            .or_else(|| token.strip_prefix("bearer "))
            .ok_or_else(|| tonic::Status::unauthenticated("bearer token required"))?;
        let claims = management_auth.verify(token)?;
        let rate_limiter = management_rate_limiter.clone();
        let caller = claims.sub.clone();
        tokio::spawn(async move {
            let _ = rate_limiter.check(&caller).await;
        });
        request.extensions_mut().insert(claims);
        Ok(request)
    };
    common::tls::validate_server_tls(&service_tls)
        .await
        .context("failed to validate server tls material")?;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .context("failed to bind gRPC listener")?;
    if service_tls.is_enabled() {
        let incoming = common::tls::dynamic_tls_incoming(listener, service_tls.clone());
        Server::builder()
            .trace_fn(common::observability::grpc_server_span)
            .add_service(KeyValidationServiceServer::with_interceptor(
                grpc,
                validation_interceptor,
            ))
            .add_service(KeyManagementServiceServer::with_interceptor(
                management,
                management_interceptor,
            ))
            .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
            .await?;
    } else {
        let incoming = common::tls::tcp_incoming(listener);
        Server::builder()
            .trace_fn(common::observability::grpc_server_span)
            .add_service(KeyValidationServiceServer::with_interceptor(
                grpc,
                validation_interceptor,
            ))
            .add_service(KeyManagementServiceServer::with_interceptor(
                management,
                management_interceptor,
            ))
            .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
            .await?;
    }
    Ok(())
}
