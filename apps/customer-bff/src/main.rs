use anyhow::{Context, Result};
use common::{AppConfig, ServiceTlsConfig};
use customer_bff::application::service::BffApplication;
use customer_bff::infrastructure::clients::{
    AuthGrpcClient, CatalogGrpcClient, FileGrpcClient, GrpcClientConfig, InvoiceGrpcClient,
    KeyGrpcClient, PaymentGrpcClient, PlanGrpcClient, UsageGrpcClient, WalletGrpcClient,
};
use customer_bff::infrastructure::rate_limiter::RedisRateLimiter;
use customer_bff::interfaces::http::{router, HttpState};
use metrics_exporter_prometheus::PrometheusBuilder;
use std::net::SocketAddr;
use std::sync::Arc;

fn string_env(key: &str) -> Result<String> {
    let value = std::env::var(key).with_context(|| format!("{key} is required"))?;
    if value.trim().is_empty() {
        anyhow::bail!("{key} cannot be empty");
    }
    Ok(value)
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env_async()
        .await
        .context("failed to load configuration")?;
    let config_reload = common::ConfigReloadHandle::start(config.clone());
    common::observability::init("customer-bff", &config)
        .context("failed to initialize observability")?;
    let service_tls = ServiceTlsConfig::from_env("customer-bff")
        .context("invalid customer-bff TLS configuration")?;
    common::startup::log_service_starting("customer-bff", &config, &service_tls);
    let jwt_secret = config
        .service_jwt_secret
        .clone()
        .context("SERVICE_JWT_SECRET is required")?;
    let issuer = string_env("CUSTOMER_BFF_SERVICE_JWT_ISSUER")?;
    let subject = string_env("CUSTOMER_BFF_SERVICE_JWT_SUBJECT")?;

    let grpc_timeout_secs = string_env("GRPC_TIMEOUT_SECS")?
        .parse::<u64>()
        .context("GRPC_TIMEOUT_SECS must be a number")?;

    let client_config = |endpoint_key: &str| -> Result<GrpcClientConfig> {
        Ok(GrpcClientConfig {
            endpoint: string_env(endpoint_key)?,
            jwt_secret: jwt_secret.clone(),
            issuer: issuer.clone(),
            subject: subject.clone(),
            timeout_secs: grpc_timeout_secs,
            tls_config: service_tls.is_enabled().then(|| service_tls.clone()),
        })
    };

    let cookie_secure = string_env("COOKIE_SECURE")?
        .parse::<bool>()
        .context("COOKIE_SECURE must be true or false")?;
    let cookie_domain = std::env::var("COOKIE_DOMAIN")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty());

    let app = BffApplication {
        auth: AuthGrpcClient::new(client_config("AUTH_SERVICE_GRPC_ENDPOINT")?),
        keys: KeyGrpcClient::new(client_config("KEY_SERVICES_GRPC_ENDPOINT")?),
        plans: PlanGrpcClient::new(client_config("SUBSCRIPTION_SERVICE_GRPC_ENDPOINT")?),
        catalog: CatalogGrpcClient::new(client_config("CATALOG_GRPC_ENDPOINT")?),
        wallet: WalletGrpcClient::new(client_config("BILLING_SERVICE_GRPC_ENDPOINT")?),
        invoices: InvoiceGrpcClient::new(client_config("INVOICE_SERVICE_GRPC_ENDPOINT")?),
        payments: PaymentGrpcClient::new(client_config("PAYMENT_SERVICES_GRPC_ENDPOINT")?),
        usage: UsageGrpcClient::new(client_config("USAGE_SERVICE_GRPC_ENDPOINT")?),
        files: FileGrpcClient::new(GrpcClientConfig {
            endpoint: std::env::var("FILE_SERVICE_GRPC_ENDPOINT")
                .unwrap_or_else(|_| "https://127.0.0.1:50071".into()),
            jwt_secret: jwt_secret.clone(),
            issuer: issuer.clone(),
            subject: subject.clone(),
            timeout_secs: grpc_timeout_secs,
            tls_config: service_tls.is_enabled().then(|| service_tls.clone()),
        }),
        cookie_secure,
        cookie_domain: cookie_domain.clone(),
    };

    let metrics_handle = PrometheusBuilder::new()
        .install_recorder()
        .context("failed to install prometheus metrics recorder")?;

    let bind_addr = string_env("CUSTOMER_BFF_BIND_ADDR")?;
    let http_port = config.http_port.context("HTTP_PORT is required")?;
    let address: SocketAddr = format!("{bind_addr}:{http_port}")
        .parse()
        .context("invalid customer-bff bind address")?;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .context("failed to bind customer-bff HTTP server")?;
    tracing::info!(%address, "customer-bff HTTP listening");

    let cors_allowed_origins = string_env("CORS_ALLOWED_ORIGINS")?
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>();
    let cookie_secure = string_env("COOKIE_SECURE")?
        .parse::<bool>()
        .context("COOKIE_SECURE must be true or false")?;

    let rate_limit_ip_rpm = string_env("RATE_LIMIT_IP_REQUESTS_PER_MINUTE")?
        .parse::<usize>()
        .context("RATE_LIMIT_IP_REQUESTS_PER_MINUTE must be a number")?;
    let rate_limit_user_rpm = string_env("RATE_LIMIT_USER_REQUESTS_PER_MINUTE")?
        .parse::<usize>()
        .context("RATE_LIMIT_USER_REQUESTS_PER_MINUTE must be a number")?;
    let redis_url = string_env("REDIS_URL")?;
    let redis_pool_size = string_env("REDIS_POOL_SIZE")?
        .parse::<usize>()
        .context("REDIS_POOL_SIZE must be a number")?;
    let redis_pool =
        common::startup::connect_redis("customer-bff", &redis_url, redis_pool_size, 2000)
            .await
            .context("failed to create Redis pool")?;
    config_reload.register_redis_pool("customer-bff", "REDIS_URL", redis_pool.clone());

    axum::serve(
        listener,
        router(HttpState {
            app: Arc::new(app),
            metrics: Arc::new(metrics_handle),
            cors_allowed_origins,
            cookie_secure,
            cookie_domain,
            rate_limiter: RedisRateLimiter::new(redis_pool, rate_limit_ip_rpm, rate_limit_user_rpm),
        })
        .into_make_service(),
    )
    .with_graceful_shutdown(common::shutdown::shutdown_signal())
    .await
    .context("customer-bff HTTP server failed")?;

    Ok(())
}
