use anyhow::{Context, Result};
use billing_service::application::service::{BillingApplication, BillingConfig};
use billing_service::infrastructure::catalog::CatalogPricingClient;
use billing_service::infrastructure::http::{router, BillingHttpState, BillingMetrics};
use billing_service::infrastructure::messaging::{
    ensure_jetstream_stream, run_payment_succeeded_consumer, run_reservation_expiration_job,
    run_usage_recorded_consumer, BillingEventPublisher,
};
use billing_service::infrastructure::persistence::PgBillingRepository;
use billing_service::interfaces::grpc::BillingGrpcService;
use common::{AppConfig, GrpcRateLimiter, ServiceJwtAuth, ServiceTlsConfig};
use proto::billing_reservation_v1::billing_reservation_service_server::BillingReservationServiceServer;
use proto::billing_reservation_v1::billing_wallet_service_server::BillingWalletServiceServer;
use rust_decimal::Decimal;
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

fn decimal_env(key: &str) -> Result<Decimal> {
    std::env::var(key)
        .with_context(|| format!("{key} is required"))?
        .parse::<Decimal>()
        .with_context(|| format!("{key} must be a valid decimal"))
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env_async()
        .await
        .context("failed to load configuration")?;
    let config_reload = common::ConfigReloadHandle::start(config.clone());
    common::observability::init("billing-service", &config)
        .context("failed to initialize observability")?;
    let service_tls = ServiceTlsConfig::from_env("billing-service")
        .context("invalid billing-service TLS configuration")?;
    common::startup::log_service_starting("billing-service", &config, &service_tls);
    let db_url = std::env::var("BILLING_SERVICE_DATABASE_URL")
        .context("BILLING_SERVICE_DATABASE_URL is required")?;
    let pool = common::startup::connect_postgres(
        "billing-service",
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
        "billing-service",
        "BILLING_SERVICE_DATABASE_URL",
        pool.clone(),
    );
    let nats = common::connect_nats(&config.nats_url)
        .await
        .context("failed to connect nats")?;
    config_reload.register_nats_client("billing-service", "NATS_URL", nats.clone());
    let service_jwt_secret = required(config.service_jwt_secret.clone(), "SERVICE_JWT_SECRET")?;
    let audit_hmac_secret =
        std::env::var("AUDIT_HMAC_SECRET").unwrap_or_else(|_| service_jwt_secret.clone());
    let repo = Arc::new(PgBillingRepository {
        pool: pool.clone(),
        signer: common::AuditSigner::new(audit_hmac_secret),
    });
    let pricing = Arc::new(
        CatalogPricingClient::new(
            string_env("BILLING_CATALOG_GRPC_ENDPOINT")?,
            service_jwt_secret.clone(),
            string_env("BILLING_SERVICE_JWT_ISSUER")?,
            string_env("BILLING_SERVICE_JWT_SUBJECT")?,
            service_tls.is_enabled().then(|| service_tls.clone()),
        )
        .await?,
    );
    let jetstream = async_nats::jetstream::new(nats);
    ensure_jetstream_stream(&jetstream)
        .await
        .context("failed to setup JetStream streams")?;
    let outbox = events::outbox::OutboxStore::new(pool.clone(), "event_outbox")
        .context("failed to configure billing outbox")?;
    let relay = events::outbox::OutboxRelay::new(
        pool,
        jetstream.clone(),
        events::outbox::OutboxConfig::for_service("billing-service", "event_outbox"),
    )
    .context("failed to configure billing outbox relay")?;
    tokio::spawn(relay.run());
    let app = Arc::new(BillingApplication {
        repo: repo.clone(),
        events: Arc::new(BillingEventPublisher {
            outbox,
            context: jetstream,
        }),
        pricing,
        config: BillingConfig {
            currency: string_env("BILLING_CURRENCY")?,
            low_balance_threshold: decimal_env("BILLING_LOW_BALANCE_THRESHOLD")?,
            reservation_ttl_minutes: i64_env("BILLING_RESERVATION_TTL_MINUTES")?,
            input_token_price_per_1k: decimal_env("BILLING_INPUT_TOKEN_PRICE_PER_1K")?,
            output_token_price_per_1k: decimal_env("BILLING_OUTPUT_TOKEN_PRICE_PER_1K")?,
        },
    });
    let redis_pool = common::startup::connect_redis(
        "billing-service",
        &config.redis_url,
        required(config.redis_pool_size, "REDIS_POOL_SIZE")?,
        required(config.redis_timeout_ms, "REDIS_TIMEOUT_MS")?,
    )
    .await
    .context("failed to create redis pool")?;
    config_reload.register_redis_pool("billing-service", "REDIS_URL", redis_pool.clone());
    let grpc_rate_limiter = Arc::new(GrpcRateLimiter::new(
        redis_pool,
        std::env::var("GRPC_RATE_LIMIT_RPM")
            .unwrap_or_else(|_| "1000".to_string())
            .parse::<u64>()
            .context("GRPC_RATE_LIMIT_RPM must be a valid u64")?,
    ));
    let mut secrets: Vec<String> = vec![service_jwt_secret];
    if let Ok(prev) = std::env::var("SERVICE_JWT_PREVIOUS_SECRET") {
        if !prev.is_empty() && prev != secrets[0] {
            secrets.push(prev);
        }
    }
    let service_auth = Arc::new(
        ServiceJwtAuth::with_secrets(
            &secrets,
            csv_env("BILLING_SERVICE_ALLOWED_ISSUERS")?,
            csv_env("BILLING_SERVICE_ALLOWED_SUBJECTS")?,
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?,
    );
    let bind_addr = std::env::var("BILLING_SERVICE_BIND_ADDR")
        .context("BILLING_SERVICE_BIND_ADDR is required")?;
    let address: SocketAddr = format!("{}:{}", bind_addr, required(config.grpc_port, "GRPC_PORT")?)
        .parse()
        .context("invalid billing-service bind address")?;
    let http_addr: SocketAddr = format!(
        "{}:{}",
        std::env::var("BILLING_SERVICE_HTTP_BIND_ADDR")
            .context("BILLING_SERVICE_HTTP_BIND_ADDR is required")?,
        std::env::var("BILLING_SERVICE_HTTP_PORT")
            .context("BILLING_SERVICE_HTTP_PORT is required")?
    )
    .parse()
    .context("invalid billing-service http bind address")?;

    let consumer_app = app.clone();
    let billing_metrics = Arc::new(BillingMetrics::default());
    let payment_metrics = billing_metrics.clone();
    tokio::spawn(async move {
        if let Err(error) = run_payment_succeeded_consumer(consumer_app, payment_metrics).await {
            tracing::error!(%error, "payment.succeeded consumer stopped");
        }
    });
    let usage_consumer_app = app.clone();
    let usage_metrics = billing_metrics.clone();
    tokio::spawn(async move {
        if let Err(error) = run_usage_recorded_consumer(usage_consumer_app, usage_metrics).await {
            tracing::error!(%error, "usage.recorded consumer stopped");
        }
    });
    tokio::spawn(run_reservation_expiration_job(app.clone()));

    let grpc = BillingGrpcService { app };
    let reservation_auth = Arc::clone(&service_auth);
    let reservation_rate_limiter = grpc_rate_limiter.clone();
    let reservation_interceptor = move |mut request: Request<()>| {
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
        let claims = reservation_auth.verify(token)?;
        let rate_limiter = reservation_rate_limiter.clone();
        let caller = claims.sub.clone();
        tokio::spawn(async move {
            let _ = rate_limiter.check(&caller).await;
        });
        request.extensions_mut().insert(claims);
        Ok(request)
    };
    let wallet_auth = Arc::clone(&service_auth);
    let wallet_rate_limiter = grpc_rate_limiter.clone();
    let wallet_interceptor = move |mut request: Request<()>| {
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
        let claims = wallet_auth.verify(token)?;
        let rate_limiter = wallet_rate_limiter.clone();
        let caller = claims.sub.clone();
        tokio::spawn(async move {
            let _ = rate_limiter.check(&caller).await;
        });
        request.extensions_mut().insert(claims);
        Ok(request)
    };

    let listener = tokio::net::TcpListener::bind(http_addr)
        .await
        .context("failed to bind billing HTTP server")?;
    let http_server = async move {
        axum::serve(
            listener,
            router(BillingHttpState {
                metrics: billing_metrics,
            }),
        )
        .await
        .context("billing HTTP server failed")
    };

    info!(%address, %http_addr, "billing-service listening");

    let grpc_server = async move {
        common::tls::validate_server_tls(&service_tls)
            .await
            .context("failed to validate server tls material")?;
        let grpc_listener = tokio::net::TcpListener::bind(address)
            .await
            .context("failed to bind billing gRPC listener")?;
        if service_tls.is_enabled() {
            let incoming = common::tls::dynamic_tls_incoming(grpc_listener, service_tls.clone());
            Server::builder()
                .trace_fn(common::observability::grpc_server_span)
                .add_service(BillingReservationServiceServer::with_interceptor(
                    grpc.clone(),
                    reservation_interceptor,
                ))
                .add_service(BillingWalletServiceServer::with_interceptor(
                    grpc,
                    wallet_interceptor,
                ))
                .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
                .await
                .context("billing gRPC server failed")?;
        } else {
            let incoming = common::tls::tcp_incoming(grpc_listener);
            Server::builder()
                .trace_fn(common::observability::grpc_server_span)
                .add_service(BillingReservationServiceServer::with_interceptor(
                    grpc.clone(),
                    reservation_interceptor,
                ))
                .add_service(BillingWalletServiceServer::with_interceptor(
                    grpc,
                    wallet_interceptor,
                ))
                .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
                .await
                .context("billing gRPC server failed")?;
        }
        Ok::<(), anyhow::Error>(())
    };
    tokio::try_join!(
        async move { grpc_server.await.context("billing gRPC server failed") },
        http_server
    )?;
    Ok(())
}
