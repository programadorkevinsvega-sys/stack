use anyhow::{Context, Result};
use common::{AppConfig, GrpcRateLimiter, ServiceJwtAuth, ServiceTlsConfig};
use payment_services::application::commands::{
    AttachPaymentMethod, CapturePayPalOrder, ChargePayPalVault, CreatePayPalPaymentToken,
    CreatePayPalVaultSetupToken, CreateTopUp, HandlePayPalWebhook, RemovePaymentMethod,
    SetDefaultPaymentMethod,
};
use payment_services::application::ports::{AuditLogger, PaymentProvider};
use payment_services::application::queries::{GetPaymentMethod, ListPaymentMethods};
use payment_services::domain::repositories::{
    PaymentMethodRepository, PaymentProviderEventRepository, PaymentTransactionRepository,
};
use payment_services::infrastructure::paypal::PayPalClient;
use payment_services::infrastructure::persistence::{
    PgAuditLogger, PgPaymentMethodRepository, PgPaymentProviderEventRepository,
    PgPaymentTransactionRepository,
};
use payment_services::interfaces::grpc::PaymentGrpcService;
use payment_services::interfaces::http::{router, PaymentHttpMetrics, PaymentHttpState};
use proto::payment_v1::payment_service_server::PaymentServiceServer;
use std::collections::HashSet;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
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

fn usize_env(key: &str) -> Result<usize> {
    std::env::var(key)
        .with_context(|| format!("{key} is required"))?
        .parse::<usize>()
        .with_context(|| format!("{key} must be a valid usize"))
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env_async()
        .await
        .context("failed to load configuration")?;
    let config_reload = common::ConfigReloadHandle::start(config.clone());
    common::observability::init("payment-services", &config)
        .context("failed to initialize observability")?;
    let service_tls = ServiceTlsConfig::from_env("payment-services")
        .context("invalid payment-services TLS configuration")?;
    common::startup::log_service_starting("payment-services", &config, &service_tls);
    let db_url = std::env::var("PAYMENT_SERVICES_DATABASE_URL")
        .context("PAYMENT_SERVICES_DATABASE_URL is required")?;
    let pool = common::startup::connect_postgres(
        "payment-services",
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
        "payment-services",
        "PAYMENT_SERVICES_DATABASE_URL",
        pool.clone(),
    );
    let service_jwt_secret = required(config.service_jwt_secret.clone(), "SERVICE_JWT_SECRET")?;
    let audit_hmac_secret =
        std::env::var("AUDIT_HMAC_SECRET").unwrap_or_else(|_| service_jwt_secret.clone());
    let mut secrets: Vec<String> = vec![service_jwt_secret.clone()];
    if let Ok(prev) = std::env::var("SERVICE_JWT_PREVIOUS_SECRET") {
        if !prev.is_empty() && prev != secrets[0] {
            secrets.push(prev);
        }
    }
    let service_auth = Arc::new(
        ServiceJwtAuth::with_secrets(
            &secrets,
            csv_env("PAYMENT_SERVICES_ALLOWED_ISSUERS")?,
            csv_env("PAYMENT_SERVICES_ALLOWED_SUBJECTS")?,
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?,
    );
    let provider: Arc<dyn PaymentProvider> = Arc::new(PayPalClient::new(
        string_env("PAYPAL_BASE_URL")?,
        string_env("PAYPAL_CLIENT_ID")?,
        string_env("PAYPAL_CLIENT_SECRET")?,
        string_env("PAYPAL_WEBHOOK_ID")?,
    )?);
    let method_repo: Arc<dyn PaymentMethodRepository> =
        Arc::new(PgPaymentMethodRepository { pool: pool.clone() });
    let provider_events: Arc<dyn PaymentProviderEventRepository> =
        Arc::new(PgPaymentProviderEventRepository { pool: pool.clone() });
    let transactions: Arc<dyn PaymentTransactionRepository> =
        Arc::new(PgPaymentTransactionRepository { pool: pool.clone() });
    let audit: Arc<dyn AuditLogger> = Arc::new(PgAuditLogger {
        pool: pool.clone(),
        signer: common::AuditSigner::new(audit_hmac_secret),
    });
    let nats = common::connect_nats(&config.nats_url)
        .await
        .context("failed to connect nats")?;
    config_reload.register_nats_client("payment-services", "NATS_URL", nats.clone());
    let jetstream = async_nats::jetstream::new(nats);
    events::jetstream::ensure_streams(&jetstream)
        .await
        .context("failed to configure JetStream streams")?;
    let relay = events::outbox::OutboxRelay::new(
        pool,
        jetstream,
        events::outbox::OutboxConfig::for_service("payment-services", "event_outbox"),
    )
    .context("failed to configure payment outbox relay")?;
    tokio::spawn(relay.run());

    let capture = CapturePayPalOrder {
        provider: provider.clone(),
        provider_events: provider_events.clone(),
        transactions: transactions.clone(),
        audit: audit.clone(),
    };
    let webhook = Arc::new(HandlePayPalWebhook {
        provider,
        provider_events: provider_events.clone(),
        transactions: transactions.clone(),
        audit: audit.clone(),
    });
    let grpc = PaymentGrpcService {
        create_top_up: CreateTopUp {
            provider: webhook.provider.clone(),
            events: provider_events.clone(),
            transactions: transactions.clone(),
            audit: audit.clone(),
        },
        capture_paypal_order: capture,
        charge_paypal_vault: ChargePayPalVault {
            provider: webhook.provider.clone(),
            methods: method_repo.clone(),
            transactions: transactions.clone(),
            provider_events: provider_events.clone(),
            audit: audit.clone(),
        },
        create_paypal_vault_setup_token: CreatePayPalVaultSetupToken {
            provider: webhook.provider.clone(),
            audit: audit.clone(),
        },
        create_paypal_payment_token: CreatePayPalPaymentToken {
            provider: webhook.provider.clone(),
            methods: method_repo.clone(),
            audit: audit.clone(),
        },
        attach_payment_method: AttachPaymentMethod {
            repo: method_repo.clone(),
            audit: audit.clone(),
        },
        remove_payment_method: RemovePaymentMethod {
            repo: method_repo.clone(),
            audit: audit.clone(),
        },
        set_default_payment_method: SetDefaultPaymentMethod {
            repo: method_repo.clone(),
            audit,
        },
        list_payment_methods: ListPaymentMethods {
            repo: method_repo.clone(),
        },
        get_payment_method: GetPaymentMethod { repo: method_repo },
    };

    let bind_addr = std::env::var("PAYMENT_SERVICES_BIND_ADDR")
        .context("PAYMENT_SERVICES_BIND_ADDR is required")?;
    let grpc_addr: SocketAddr =
        format!("{}:{}", bind_addr, required(config.grpc_port, "GRPC_PORT")?)
            .parse()
            .context("invalid payment-services gRPC bind address")?;
    let http_addr: SocketAddr = format!(
        "{}:{}",
        std::env::var("PAYMENT_SERVICES_WEBHOOK_BIND_ADDR")
            .context("PAYMENT_SERVICES_WEBHOOK_BIND_ADDR is required")?,
        std::env::var("PAYMENT_SERVICES_WEBHOOK_HTTP_PORT")
            .context("PAYMENT_SERVICES_WEBHOOK_HTTP_PORT is required")?
    )
    .parse()
    .context("invalid payment-services webhook bind address")?;

    let redis_pool = common::startup::connect_redis(
        "payment-services",
        &config.redis_url,
        required(config.redis_pool_size, "REDIS_POOL_SIZE")?,
        required(config.redis_timeout_ms, "REDIS_TIMEOUT_MS")?,
    )
    .await
    .context("failed to create redis pool")?;
    config_reload.register_redis_pool("payment-services", "REDIS_URL", redis_pool.clone());
    let grpc_rate_limiter = Arc::new(GrpcRateLimiter::new(
        redis_pool.clone(),
        std::env::var("GRPC_RATE_LIMIT_RPM")
            .unwrap_or_else(|_| "1000".to_string())
            .parse::<u64>()
            .context("GRPC_RATE_LIMIT_RPM must be a valid u64")?,
    ));

    let grpc_auth = Arc::clone(&service_auth);
    let grpc_rate_limiter_clone = grpc_rate_limiter.clone();
    let grpc_interceptor = move |mut request: Request<()>| {
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
        let claims = grpc_auth.verify(token)?;
        let rate_limiter = grpc_rate_limiter_clone.clone();
        let caller = claims.sub.clone();
        tokio::spawn(async move {
            let _ = rate_limiter.check(&caller).await;
        });
        request.extensions_mut().insert(claims);
        Ok(request)
    };

    let grpc_server = async move {
        common::tls::validate_server_tls(&service_tls)
            .await
            .context("failed to validate server tls material")?;
        let grpc_listener = tokio::net::TcpListener::bind(grpc_addr)
            .await
            .context("failed to bind payment gRPC listener")?;
        if service_tls.is_enabled() {
            let incoming = common::tls::dynamic_tls_incoming(grpc_listener, service_tls.clone());
            Server::builder()
                .trace_fn(common::observability::grpc_server_span)
                .add_service(PaymentServiceServer::with_interceptor(
                    grpc,
                    grpc_interceptor,
                ))
                .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
                .await
                .context("payment gRPC server failed")
        } else {
            let incoming = common::tls::tcp_incoming(grpc_listener);
            Server::builder()
                .trace_fn(common::observability::grpc_server_span)
                .add_service(PaymentServiceServer::with_interceptor(
                    grpc,
                    grpc_interceptor,
                ))
                .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
                .await
                .context("payment gRPC server failed")
        }
    };

    let listener = tokio::net::TcpListener::bind(http_addr)
        .await
        .context("failed to bind payment webhook HTTP server")?;
    let http_server = async move {
        axum::serve(
            listener,
            router(
                PaymentHttpState {
                    paypal_webhook: webhook,
                    redis: redis_pool,
                    max_requests_per_minute: usize_env(
                        "PAYMENT_SERVICES_WEBHOOK_RATE_LIMIT_PER_MINUTE",
                    )?,
                    metrics: Arc::new(PaymentHttpMetrics::default()),
                },
                usize_env("PAYMENT_SERVICES_WEBHOOK_MAX_BODY_BYTES")?,
                Duration::from_millis(usize_env("PAYMENT_SERVICES_WEBHOOK_TIMEOUT_MS")? as u64),
            ),
        )
        .await
        .context("payment webhook HTTP server failed")
    };

    info!(%grpc_addr, %http_addr, "payment-services listening");
    tokio::try_join!(grpc_server, http_server)?;
    Ok(())
}
