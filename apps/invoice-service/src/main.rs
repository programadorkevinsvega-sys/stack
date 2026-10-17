use anyhow::{Context, Result};
use aws_config::BehaviorVersion;
use aws_credential_types::Credentials;
use aws_sdk_s3::config::Region;
use common::{AppConfig, GrpcRateLimiter, ServiceJwtAuth, ServiceTlsConfig};
use invoice_service::application::service::{InvoiceApplication, InvoiceConfig};
use invoice_service::infrastructure::http::{router, InvoiceHttpState, InvoiceMetrics};
use invoice_service::infrastructure::messaging::{
    ensure_jetstream_stream, run_billing_period_closed_consumer, InvoiceEventBus,
};
use invoice_service::infrastructure::pdf::{MinimalPdfGenerator, R2PdfStorage};
use invoice_service::infrastructure::persistence::PgInvoiceRepository;
use invoice_service::interfaces::grpc::InvoiceGrpcService;
use proto::invoice_v1::invoice_service_server::InvoiceServiceServer;
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

fn decimal_env(key: &str) -> Result<Decimal> {
    std::env::var(key)
        .with_context(|| format!("{key} is required"))?
        .parse::<Decimal>()
        .with_context(|| format!("{key} must be a valid decimal"))
}

fn i64_env(key: &str) -> Result<i64> {
    std::env::var(key)
        .with_context(|| format!("{key} is required"))?
        .parse::<i64>()
        .with_context(|| format!("{key} must be a valid i64"))
}

fn r2_endpoint_url() -> Result<String> {
    if let Ok(value) = std::env::var("INVOICE_R2_ENDPOINT_URL") {
        let trimmed = value.trim();
        if !trimmed.is_empty() {
            return Ok(trimmed.trim_end_matches('/').to_owned());
        }
    }
    let account_id = string_env("INVOICE_R2_ACCOUNT_ID")?;
    Ok(format!(
        "https://{}.r2.cloudflarestorage.com",
        account_id.trim()
    ))
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env_async()
        .await
        .context("failed to load configuration")?;
    let config_reload = common::ConfigReloadHandle::start(config.clone());
    common::observability::init("invoice-service", &config)
        .context("failed to initialize observability")?;
    let service_tls = ServiceTlsConfig::from_env("invoice-service")
        .context("invalid invoice-service TLS configuration")?;
    common::startup::log_service_starting("invoice-service", &config, &service_tls);
    let db_url = std::env::var("INVOICE_SERVICE_DATABASE_URL")
        .context("INVOICE_SERVICE_DATABASE_URL is required")?;
    let pool = common::startup::connect_postgres(
        "invoice-service",
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
        "invoice-service",
        "INVOICE_SERVICE_DATABASE_URL",
        pool.clone(),
    );
    let nats = common::connect_nats(&config.nats_url)
        .await
        .context("failed to connect nats")?;
    config_reload.register_nats_client("invoice-service", "NATS_URL", nats.clone());
    let redis_pool = common::startup::connect_redis(
        "invoice-service",
        &config.redis_url,
        required(config.redis_pool_size, "REDIS_POOL_SIZE")?,
        required(config.redis_timeout_ms, "REDIS_TIMEOUT_MS")?,
    )
    .await
    .context("failed to create redis pool")?;
    config_reload.register_redis_pool("invoice-service", "REDIS_URL", redis_pool.clone());
    let grpc_rate_limiter = Arc::new(GrpcRateLimiter::new(
        redis_pool,
        std::env::var("GRPC_RATE_LIMIT_RPM")
            .unwrap_or_else(|_| "1000".to_string())
            .parse::<u64>()
            .context("GRPC_RATE_LIMIT_RPM must be a valid u64")?,
    ));
    let service_jwt_secret = required(config.service_jwt_secret.clone(), "SERVICE_JWT_SECRET")?;
    let audit_hmac_secret =
        std::env::var("AUDIT_HMAC_SECRET").unwrap_or_else(|_| service_jwt_secret.clone());
    let r2_endpoint = r2_endpoint_url()?;
    let r2_bucket = string_env("INVOICE_R2_BUCKET")?;
    let r2_use_iam = std::env::var("INVOICE_R2_USE_IAM")
        .unwrap_or_default()
        .to_lowercase()
        == "true";
    let r2_config = if r2_use_iam {
        aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(&r2_endpoint)
            .region(Region::new("auto"))
            .load()
            .await
    } else {
        let r2_credentials = Credentials::new(
            string_env("INVOICE_R2_ACCESS_KEY_ID")?,
            string_env("INVOICE_R2_SECRET_ACCESS_KEY")?,
            None,
            None,
            "invoice-service-r2",
        );
        aws_config::defaults(BehaviorVersion::latest())
            .endpoint_url(&r2_endpoint)
            .region(Region::new("auto"))
            .credentials_provider(r2_credentials)
            .load()
            .await
    };
    let r2_client = aws_sdk_s3::Client::new(&r2_config);
    info!(
        service = "invoice-service",
        dependency = "r2",
        endpoint = %common::startup::sanitize_endpoint(&r2_endpoint),
        bucket = %r2_bucket,
        authentication = if r2_use_iam { "iam" } else { "static_credentials" },
        "checking dependency"
    );
    match r2_client.head_bucket().bucket(&r2_bucket).send().await {
        Ok(_) => {
            info!(
                service = "invoice-service",
                dependency = "r2",
                endpoint = %common::startup::sanitize_endpoint(&r2_endpoint),
                bucket = %r2_bucket,
                "dependency connected"
            );
        }
        Err(error) => {
            tracing::error!(
                service = "invoice-service",
                dependency = "r2",
                endpoint = %common::startup::sanitize_endpoint(&r2_endpoint),
                bucket = %r2_bucket,
                status = "unavailable",
                http_status = error
                    .raw_response()
                    .map(|response| response.status().as_u16()),
                error = ?error,
                "dependency unavailable; service is starting in degraded mode"
            );
        }
    }
    let app = Arc::new(InvoiceApplication {
        repo: Arc::new(PgInvoiceRepository {
            pool,
            signer: common::AuditSigner::new(audit_hmac_secret),
        }),
        pdf_generator: Arc::new(MinimalPdfGenerator),
        pdf_storage: Arc::new(R2PdfStorage {
            client: r2_client,
            bucket: r2_bucket,
            prefix: string_env("INVOICE_R2_PREFIX")?,
        }),
        config: InvoiceConfig {
            currency: string_env("INVOICE_CURRENCY")?,
            tax_rate: decimal_env("INVOICE_TAX_RATE")?,
            due_days: i64_env("INVOICE_DUE_DAYS")?,
        },
    });
    let mut secrets: Vec<String> = vec![service_jwt_secret];
    if let Ok(prev) = std::env::var("SERVICE_JWT_PREVIOUS_SECRET") {
        if !prev.is_empty() && prev != secrets[0] {
            secrets.push(prev);
        }
    }
    let service_auth = Arc::new(
        ServiceJwtAuth::with_secrets(
            &secrets,
            csv_env("INVOICE_SERVICE_ALLOWED_ISSUERS")?,
            csv_env("INVOICE_SERVICE_ALLOWED_SUBJECTS")?,
        )
        .map_err(|e| anyhow::anyhow!("{e}"))?,
    );
    let bind_addr = std::env::var("INVOICE_SERVICE_BIND_ADDR")
        .context("INVOICE_SERVICE_BIND_ADDR is required")?;
    let address: SocketAddr = format!("{}:{}", bind_addr, required(config.grpc_port, "GRPC_PORT")?)
        .parse()
        .context("invalid invoice-service bind address")?;
    let http_addr: SocketAddr = format!(
        "{}:{}",
        std::env::var("INVOICE_SERVICE_HTTP_BIND_ADDR")
            .context("INVOICE_SERVICE_HTTP_BIND_ADDR is required")?,
        std::env::var("INVOICE_SERVICE_HTTP_PORT")
            .context("INVOICE_SERVICE_HTTP_PORT is required")?
    )
    .parse()
    .context("invalid invoice-service http bind address")?;

    let metrics = Arc::new(InvoiceMetrics::default());
    let jetstream = async_nats::jetstream::new(nats);
    ensure_jetstream_stream(&jetstream)
        .await
        .context("failed to setup jetstream stream")?;
    tokio::spawn(run_billing_period_closed_consumer(
        app.clone(),
        InvoiceEventBus { context: jetstream },
        metrics.clone(),
    ));

    let grpc = InvoiceGrpcService {
        app,
        admin_callers: csv_env("INVOICE_SERVICE_ADMIN_SUBJECTS")?,
        user_access_callers: csv_env("INVOICE_SERVICE_USER_ACCESS_SUBJECTS")?,
    };
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
        .context("failed to bind invoice HTTP server")?;
    let http_server = async move {
        axum::serve(listener, router(InvoiceHttpState { metrics }))
            .await
            .context("invoice HTTP server failed")
    };

    info!(%address, %http_addr, "invoice-service listening");

    let grpc_server = async move {
        common::tls::validate_server_tls(&service_tls)
            .await
            .context("failed to validate server tls material")?;
        let grpc_listener = tokio::net::TcpListener::bind(address)
            .await
            .context("failed to bind invoice gRPC listener")?;
        if service_tls.is_enabled() {
            let incoming = common::tls::dynamic_tls_incoming(grpc_listener, service_tls.clone());
            Server::builder()
                .trace_fn(common::observability::grpc_server_span)
                .add_service(InvoiceServiceServer::with_interceptor(grpc, interceptor))
                .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
                .await
                .context("invoice gRPC server failed")?;
        } else {
            let incoming = common::tls::tcp_incoming(grpc_listener);
            Server::builder()
                .trace_fn(common::observability::grpc_server_span)
                .add_service(InvoiceServiceServer::with_interceptor(grpc, interceptor))
                .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
                .await
                .context("invoice gRPC server failed")?;
        }
        Ok::<(), anyhow::Error>(())
    };
    tokio::try_join!(
        async move { grpc_server.await.context("invoice gRPC server failed") },
        http_server
    )?;
    Ok(())
}
