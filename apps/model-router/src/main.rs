use anyhow::{Context, Result};
use common::{AppConfig, ServiceJwtAuth, ServiceTlsConfig};
use model_router::application::service::{ModelRouterApplication, ModelRouterConfig};
use model_router::infrastructure::clients::{
    CatalogClient, CredentialManagerClient, ServiceGrpcClientConfig,
};
use model_router::infrastructure::events::RouterEventPublisher;
use model_router::infrastructure::http::{router, ModelRouterHttpState, ModelRouterMetrics};
use model_router::infrastructure::redis_state::RedisRouterState;
use model_router::interfaces::grpc::ModelRouterGrpcService;
use proto::account_selection_v1::account_selection_service_server::AccountSelectionServiceServer;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use tonic::transport::Server;
use tonic::Request;
use tracing::info;

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

fn string_env(key: &str) -> Result<String> {
    let value = std::env::var(key).with_context(|| format!("{key} is required"))?;
    if value.trim().is_empty() {
        anyhow::bail!("{key} cannot be empty");
    }
    Ok(value)
}

fn i64_env(key: &str) -> Result<i64> {
    std::env::var(key)
        .with_context(|| format!("{key} is required"))?
        .parse::<i64>()
        .with_context(|| format!("{key} must be a valid i64"))
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env_async()
        .await
        .context("failed to load configuration")?;
    let config_reload = common::ConfigReloadHandle::start(config.clone());
    common::observability::init("model-router", &config)
        .context("failed to initialize observability")?;
    let service_tls = ServiceTlsConfig::from_env("model-router")
        .context("invalid model-router TLS configuration")?;
    common::startup::log_service_starting("model-router", &config, &service_tls);
    let router_database_url =
        std::env::var("MODEL_ROUTER_DATABASE_URL").unwrap_or_else(|_| config.database_url.clone());
    let pool = common::startup::connect_postgres(
        "model-router",
        &router_database_url,
        required(config.database_max_connections, "DATABASE_MAX_CONNECTIONS")?,
        required(
            config.database_acquire_timeout_ms,
            "DATABASE_ACQUIRE_TIMEOUT_MS",
        )?,
        required(config.database_idle_timeout_ms, "DATABASE_IDLE_TIMEOUT_MS")?,
    )
    .await
    .context("failed to connect model-router outbox database")?;
    config_reload.register_postgres_pool("model-router", "MODEL_ROUTER_DATABASE_URL", pool.clone());
    let jwt_secret = required(config.service_jwt_secret.clone(), "SERVICE_JWT_SECRET")?;
    let redis_pool_size = required(config.redis_pool_size, "REDIS_POOL_SIZE")?;
    let redis_timeout = required(config.redis_timeout_ms, "REDIS_TIMEOUT_MS")?;
    let redis = common::startup::connect_redis(
        "model-router",
        &config.redis_url,
        redis_pool_size,
        redis_timeout,
    )
    .await
    .context("failed to create redis pool")?;
    config_reload.register_redis_pool("model-router", "REDIS_URL", redis.clone());
    let nats = common::connect_nats(&config.nats_url)
        .await
        .context("failed to connect nats")?;
    config_reload.register_nats_client("model-router", "NATS_URL", nats.clone());
    let jetstream = async_nats::jetstream::new(nats);
    events::jetstream::ensure_streams(&jetstream)
        .await
        .context("failed to configure JetStream streams")?;
    let outbox = events::outbox::OutboxStore::new(pool.clone(), "event_outbox")
        .context("failed to configure model-router outbox")?;
    let relay = events::outbox::OutboxRelay::new(
        pool,
        jetstream,
        events::outbox::OutboxConfig::for_service("model-router", "event_outbox"),
    )
    .context("failed to configure model-router outbox relay")?;
    tokio::spawn(relay.run());

    let issuer = string_env("MODEL_ROUTER_SERVICE_JWT_ISSUER")?;
    let subject = string_env("MODEL_ROUTER_SERVICE_JWT_SUBJECT")?;
    let tls_config = service_tls.is_enabled().then(|| service_tls.clone());
    let app = Arc::new(ModelRouterApplication {
        credentials: Arc::new(CredentialManagerClient {
            config: ServiceGrpcClientConfig {
                endpoint: string_env("MODEL_ROUTER_CREDENTIAL_MANAGER_GRPC_ENDPOINT")?,
                jwt_secret: jwt_secret.clone(),
                issuer: issuer.clone(),
                subject: subject.clone(),
                tls_config: tls_config.clone(),
            },
        }),
        catalog: Arc::new(CatalogClient {
            config: ServiceGrpcClientConfig {
                endpoint: string_env("MODEL_ROUTER_CATALOG_GRPC_ENDPOINT")?,
                jwt_secret: jwt_secret.clone(),
                issuer,
                subject,
                tls_config,
            },
        }),
        state: Arc::new(RedisRouterState { pool: redis }),
        events: Arc::new(RouterEventPublisher { outbox }),
        config: ModelRouterConfig {
            circuit_failure_threshold: i64_env("MODEL_ROUTER_CIRCUIT_FAILURE_THRESHOLD")?,
            circuit_cooldown_seconds: i64_env("MODEL_ROUTER_CIRCUIT_COOLDOWN_SECONDS")?,
        },
    });
    let mut secrets: Vec<String> = vec![required(config.service_jwt_secret, "SERVICE_JWT_SECRET")?];
    if let Ok(prev) = std::env::var("SERVICE_JWT_PREVIOUS_SECRET") {
        if !prev.is_empty() && prev != secrets[0] {
            secrets.push(prev);
        }
    }
    let service_auth = Arc::new(
        ServiceJwtAuth::with_secrets(
            &secrets,
            csv_env("MODEL_ROUTER_ALLOWED_ISSUERS")?,
            csv_env("MODEL_ROUTER_ALLOWED_SUBJECTS")?,
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?,
    );
    let bind_addr =
        std::env::var("MODEL_ROUTER_BIND_ADDR").context("MODEL_ROUTER_BIND_ADDR is required")?;
    let address: SocketAddr = format!("{}:{}", bind_addr, required(config.grpc_port, "GRPC_PORT")?)
        .parse()
        .context("invalid model-router bind address")?;
    let http_addr: SocketAddr = format!(
        "{}:{}",
        std::env::var("MODEL_ROUTER_HTTP_BIND_ADDR")
            .context("MODEL_ROUTER_HTTP_BIND_ADDR is required")?,
        std::env::var("MODEL_ROUTER_HTTP_PORT").context("MODEL_ROUTER_HTTP_PORT is required")?
    )
    .parse()
    .context("invalid model-router http bind address")?;

    let grpc = ModelRouterGrpcService {
        app,
        admin_callers: csv_env("MODEL_ROUTER_ADMIN_SUBJECTS")?,
    };
    let auth = Arc::clone(&service_auth);
    let interceptor = move |request: Request<()>| auth.intercept(request);

    let listener = tokio::net::TcpListener::bind(http_addr)
        .await
        .context("failed to bind model-router HTTP server")?;
    let http_server = async move {
        axum::serve(
            listener,
            router(ModelRouterHttpState {
                metrics: Arc::new(ModelRouterMetrics::default()),
            }),
        )
        .await
        .context("model-router HTTP server failed")
    };

    info!(%address, %http_addr, "model-router listening");

    let grpc_server = async move {
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
                .add_service(AccountSelectionServiceServer::with_interceptor(
                    grpc,
                    interceptor,
                ))
                .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
                .await
                .context("model-router gRPC server failed")?;
        } else {
            let incoming = common::tls::tcp_incoming(listener);
            Server::builder()
                .trace_fn(common::observability::grpc_server_span)
                .add_service(AccountSelectionServiceServer::with_interceptor(
                    grpc,
                    interceptor,
                ))
                .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
                .await
                .context("model-router gRPC server failed")?;
        }
        Ok::<(), anyhow::Error>(())
    };
    tokio::try_join!(grpc_server, http_server)?;
    Ok(())
}
