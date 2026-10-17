use anyhow::{Context, Result};
use common::{AppConfig, GrpcRateLimiter, ServiceJwtAuth, ServiceTlsConfig};
use proto::usage_v1::usage_reporting_service_server::UsageReportingServiceServer;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tonic::transport::Server;
use tonic::Request;
use tracing::info;
use usage_service::application::service::UsageApplication;
use usage_service::infrastructure::http::{router, UsageHttpState, UsageMetrics};
use usage_service::infrastructure::messaging::{
    ensure_jetstream_stream, run_materialized_view_refresh_job, run_partition_management_job,
    run_usage_recorded_consumer, UsageEventBus,
};
use usage_service::infrastructure::partition_archiver::{PartitionManager, R2PartitionArchiver};
use usage_service::infrastructure::persistence::PgUsageRepository;
use usage_service::interfaces::grpc::UsageGrpcService;

fn required<T>(value: Option<T>, name: &str) -> Result<T> {
    value.with_context(|| format!("{name} is required"))
}

fn csv_env(key: &str) -> Result<HashSet<String>> {
    let parsed = std::env::var(key)
        .with_context(|| format!("{key} is required"))?
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .collect::<HashSet<_>>();
    if parsed.is_empty() {
        anyhow::bail!("{key} cannot be empty");
    }
    Ok(parsed)
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env_async()
        .await
        .context("failed to load configuration")?;
    let config_reload = common::ConfigReloadHandle::start(config.clone());
    common::observability::init("usage-service", &config)
        .context("failed to initialize observability")?;
    let service_tls = ServiceTlsConfig::from_env("usage-service")
        .context("invalid usage-service TLS configuration")?;
    common::startup::log_service_starting("usage-service", &config, &service_tls);
    let db_url = std::env::var("USAGE_SERVICE_DATABASE_URL")
        .context("USAGE_SERVICE_DATABASE_URL is required")?;
    let pool = common::startup::connect_postgres(
        "usage-service",
        &db_url,
        required(config.database_max_connections, "DATABASE_MAX_CONNECTIONS")?,
        required(
            config.database_acquire_timeout_ms,
            "DATABASE_ACQUIRE_TIMEOUT_MS",
        )?,
        required(config.database_idle_timeout_ms, "DATABASE_IDLE_TIMEOUT_MS")?,
    )
    .await
    .context("failed to connect postgres")?;
    config_reload.register_postgres_pool(
        "usage-service",
        "USAGE_SERVICE_DATABASE_URL",
        pool.clone(),
    );
    let nats = common::connect_nats(&config.nats_url)
        .await
        .context("failed to connect nats")?;
    config_reload.register_nats_client("usage-service", "NATS_URL", nats.clone());
    let app = Arc::new(UsageApplication {
        repo: Arc::new(PgUsageRepository { pool: pool.clone() }),
    });
    let redis_pool = common::startup::connect_redis(
        "usage-service",
        &config.redis_url,
        required(config.redis_pool_size, "REDIS_POOL_SIZE")?,
        required(config.redis_timeout_ms, "REDIS_TIMEOUT_MS")?,
    )
    .await
    .context("failed to create redis pool")?;
    config_reload.register_redis_pool("usage-service", "REDIS_URL", redis_pool.clone());
    let mut secrets: Vec<String> = vec![required(config.service_jwt_secret, "SERVICE_JWT_SECRET")?];
    if let Ok(prev) = std::env::var("SERVICE_JWT_PREVIOUS_SECRET") {
        if !prev.is_empty() && prev != secrets[0] {
            secrets.push(prev);
        }
    }
    let service_auth = Arc::new(
        ServiceJwtAuth::with_secrets(
            &secrets,
            csv_env("USAGE_SERVICE_ALLOWED_ISSUERS")?,
            csv_env("USAGE_SERVICE_ALLOWED_SUBJECTS")?,
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?,
    );
    let bind_addr =
        std::env::var("USAGE_SERVICE_BIND_ADDR").context("USAGE_SERVICE_BIND_ADDR is required")?;
    let address: SocketAddr = format!("{}:{}", bind_addr, required(config.grpc_port, "GRPC_PORT")?)
        .parse()
        .context("invalid usage-service bind address")?;
    let http_addr: SocketAddr = format!(
        "{}:{}",
        std::env::var("USAGE_SERVICE_HTTP_BIND_ADDR")
            .context("USAGE_SERVICE_HTTP_BIND_ADDR is required")?,
        std::env::var("USAGE_SERVICE_HTTP_PORT").context("USAGE_SERVICE_HTTP_PORT is required")?
    )
    .parse()
    .context("invalid usage-service http bind address")?;

    let metrics = Arc::new(UsageMetrics::default());
    let bus = UsageEventBus {
        context: async_nats::jetstream::new(nats),
    };
    ensure_jetstream_stream(&bus.context)
        .await
        .context("failed to setup jetstream stream")?;
    tokio::spawn(run_usage_recorded_consumer(
        app.clone(),
        bus,
        metrics.clone(),
    ));
    tokio::spawn(run_partition_management_job(app.clone()));
    tokio::spawn(run_materialized_view_refresh_job(app.clone()));

    let r2_endpoint = std::env::var("USAGE_SERVICE_R2_ENDPOINT_URL")
        .ok()
        .filter(|v| !v.is_empty());
    if let Some(endpoint) = r2_endpoint {
        let r2_access_key = std::env::var("USAGE_SERVICE_R2_ACCESS_KEY_ID")
            .context("USAGE_SERVICE_R2_ACCESS_KEY_ID required when R2 is enabled")?;
        let r2_secret_key = std::env::var("USAGE_SERVICE_R2_SECRET_ACCESS_KEY")
            .context("USAGE_SERVICE_R2_SECRET_ACCESS_KEY required when R2 is enabled")?;
        let r2_bucket = std::env::var("USAGE_SERVICE_R2_BUCKET")
            .unwrap_or_else(|_| "ai-gateway-invoices".to_string());
        let retention = std::env::var("USAGE_SERVICE_ARCHIVE_RETENTION_MONTHS")
            .unwrap_or_else(|_| "6".to_string())
            .parse::<u32>()
            .context("USAGE_SERVICE_ARCHIVE_RETENTION_MONTHS must be a valid u32")?;
        let archiver =
            R2PartitionArchiver::new(pool.clone(), &endpoint, &r2_access_key, &r2_secret_key).await;
        tracing::info!(
            service = "usage-service",
            dependency = "r2",
            endpoint = %common::startup::sanitize_endpoint(&endpoint),
            bucket = %r2_bucket,
            authentication = "static_credentials",
            "checking dependency"
        );
        match archiver.verify_bucket(&r2_bucket).await {
            Ok(()) => {
                tracing::info!(
                    service = "usage-service",
                    dependency = "r2",
                    endpoint = %common::startup::sanitize_endpoint(&endpoint),
                    bucket = %r2_bucket,
                    "dependency connected"
                );
            }
            Err(error) => {
                tracing::error!(
                    service = "usage-service",
                    dependency = "r2",
                    endpoint = %common::startup::sanitize_endpoint(&endpoint),
                    bucket = %r2_bucket,
                    status = "unavailable",
                    error = %error,
                    "optional dependency unavailable; service is starting in degraded mode"
                );
            }
        }
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(24 * 60 * 60));
            loop {
                interval.tick().await;
                match archiver.archive_old_partitions(retention, &r2_bucket).await {
                    Ok(archived) => {
                        if !archived.is_empty() {
                            tracing::info!(
                                count = archived.len(),
                                ?archived,
                                "old partitions archived to R2"
                            );
                        }
                    }
                    Err(e) => tracing::error!(error = %e, "failed to archive old partitions"),
                }
            }
        });
    } else {
        tracing::warn!(
            service = "usage-service",
            dependency = "r2",
            status = "disabled",
            "usage partition archival is disabled"
        );
    }

    let grpc = UsageGrpcService {
        app,
        admin_callers: csv_env("USAGE_SERVICE_ADMIN_SUBJECTS")?,
        user_access_callers: csv_env("USAGE_SERVICE_USER_ACCESS_SUBJECTS")?,
        aggregate_access_callers: csv_env("USAGE_SERVICE_AGGREGATE_ACCESS_SUBJECTS")?,
    };
    let grpc_rate_limiter = Arc::new(GrpcRateLimiter::new(
        redis_pool,
        std::env::var("GRPC_RATE_LIMIT_RPM")
            .unwrap_or_else(|_| "1000".to_string())
            .parse::<u64>()
            .context("GRPC_RATE_LIMIT_RPM must be a valid u64")?,
    ));
    let auth = Arc::clone(&service_auth);
    let rate_limiter = grpc_rate_limiter.clone();
    let interceptor = move |mut request: Request<()>| {
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
        let claims = auth.verify(token)?;
        let rate_limiter = rate_limiter.clone();
        let caller = claims.sub.clone();
        tokio::spawn(async move {
            let _ = rate_limiter.check(&caller).await;
        });
        request.extensions_mut().insert(claims);
        Ok(request)
    };

    let listener = tokio::net::TcpListener::bind(http_addr)
        .await
        .context("failed to bind usage HTTP server")?;
    let http_server = async move {
        axum::serve(listener, router(UsageHttpState { metrics }))
            .await
            .context("usage HTTP server failed")
    };

    info!(%address, %http_addr, "usage-service listening");

    let grpc_server = async move {
        common::tls::validate_server_tls(&service_tls)
            .await
            .context("failed to validate server tls material")?;
        let grpc_listener = tokio::net::TcpListener::bind(address)
            .await
            .context("failed to bind usage gRPC listener")?;
        if service_tls.is_enabled() {
            let incoming = common::tls::dynamic_tls_incoming(grpc_listener, service_tls.clone());
            Server::builder()
                .trace_fn(common::observability::grpc_server_span)
                .add_service(UsageReportingServiceServer::with_interceptor(
                    grpc,
                    interceptor,
                ))
                .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
                .await
                .context("usage gRPC server failed")?;
        } else {
            let incoming = common::tls::tcp_incoming(grpc_listener);
            Server::builder()
                .trace_fn(common::observability::grpc_server_span)
                .add_service(UsageReportingServiceServer::with_interceptor(
                    grpc,
                    interceptor,
                ))
                .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
                .await
                .context("usage gRPC server failed")?;
        }
        Ok::<(), anyhow::Error>(())
    };
    tokio::try_join!(
        async move { grpc_server.await.context("usage gRPC server failed") },
        http_server
    )?;
    Ok(())
}
