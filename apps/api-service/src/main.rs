use anyhow::{Context, Result};
use api_service::application::commands::{
    DeprecateModel, RegisterModel, RegisterModelProvider, SetModelPricing, SetProviderPolicy,
    UpdateModelCapabilities, UpsertModelMetadata,
};
use api_service::application::queries::{
    GetCurrentPricing, GetModel, GetModelByPublicName, GetModelMetadata, GetPricingAt,
    GetProviderPolicy, ListModels, ListPricingHistory, ListProviders,
};
use api_service::infrastructure::persistence::{
    PgAuditLogger, PgModelCatalogRepository, PgModelMetadataRepository, PgModelPricingRepository,
    PgModelProviderRepository, PgProviderPolicyRepository,
};
use api_service::interfaces::grpc::CatalogGrpcService;
use common::{AppConfig, GrpcRateLimiter, ServiceJwtAuth, ServiceTlsConfig};
use proto::catalog_v1::catalog_service_server::CatalogServiceServer;
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::Arc;
use tonic::transport::Server;
use tonic::Request;
use tracing::info;

fn parse_csv_env(key: &str) -> Result<HashSet<String>> {
    let value = std::env::var(key).with_context(|| format!("{key} is required"))?;
    let parsed: HashSet<String> = value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if parsed.is_empty() {
        anyhow::bail!("{key} must contain at least one value");
    }
    Ok(parsed)
}

fn require_option<T>(value: Option<T>, name: &str) -> Result<T> {
    value.with_context(|| format!("{name} is required"))
}

async fn ensure_required_relations(pool: &PgPool) -> Result<()> {
    let required_relations = [
        "model_providers",
        "model_catalog",
        "model_pricing",
        "plan_model_access",
        "model_metadata",
        "provider_policies",
        "audit_logs",
    ];

    for relation in required_relations {
        let exists = sqlx::query_scalar::<_, Option<String>>("SELECT to_regclass($1)::text")
            .bind(relation)
            .fetch_one(pool)
            .await
            .with_context(|| format!("failed checking relation existence: {relation}"))?;

        if exists.is_none() {
            anyhow::bail!("required relation not found: {relation}");
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env_async()
        .await
        .context("failed to load env config")?;
    let config_reload = common::ConfigReloadHandle::start(config.clone());
    common::observability::init("api-service", &config)
        .context("failed to initialize observability")?;
    let service_tls = ServiceTlsConfig::from_env("api-service")
        .context("invalid api-service TLS configuration")?;
    common::startup::log_service_starting("api-service", &config, &service_tls);

    let service_jwt_secret =
        require_option(config.service_jwt_secret.clone(), "SERVICE_JWT_SECRET")?;
    let allowed_issuers = parse_csv_env("API_SERVICE_ALLOWED_ISSUERS")?;
    let allowed_subjects = parse_csv_env("API_SERVICE_ALLOWED_SUBJECTS")?;
    let admin_callers = parse_csv_env("API_SERVICE_ADMIN_SUBJECTS")?;
    if admin_callers.is_empty() {
        anyhow::bail!("API_SERVICE_ADMIN_SUBJECTS cannot be empty");
    }
    if !admin_callers.is_subset(&allowed_subjects) {
        let unexpected: Vec<String> = admin_callers
            .difference(&allowed_subjects)
            .cloned()
            .collect();
        anyhow::bail!(
            "API_SERVICE_ADMIN_SUBJECTS must be subset of API_SERVICE_ALLOWED_SUBJECTS; unexpected: {}",
            unexpected.join(",")
        );
    }
    let bind_addr =
        std::env::var("API_SERVICE_BIND_ADDR").context("API_SERVICE_BIND_ADDR is required")?;

    let db_max_connections =
        require_option(config.database_max_connections, "DATABASE_MAX_CONNECTIONS")?;
    let db_acquire_timeout_ms = require_option(
        config.database_acquire_timeout_ms,
        "DATABASE_ACQUIRE_TIMEOUT_MS",
    )?;
    let db_idle_timeout_ms =
        require_option(config.database_idle_timeout_ms, "DATABASE_IDLE_TIMEOUT_MS")?;
    let grpc_port = require_option(config.grpc_port, "GRPC_PORT")?;

    let db_url = std::env::var("API_SERVICE_DATABASE_URL")
        .context("API_SERVICE_DATABASE_URL is required")?;
    let pool = common::startup::connect_postgres(
        "api-service",
        &db_url,
        db_max_connections,
        db_acquire_timeout_ms,
        db_idle_timeout_ms,
    )
    .await
    .context("failed to connect postgres")?;
    config_reload.register_postgres_pool("api-service", "API_SERVICE_DATABASE_URL", pool.clone());

    ensure_required_relations(&pool).await?;
    info!("Required database relations verified");

    let redis_pool = common::startup::connect_redis(
        "api-service",
        &config.redis_url,
        require_option(config.redis_pool_size, "REDIS_POOL_SIZE")?,
        require_option(config.redis_timeout_ms, "REDIS_TIMEOUT_MS")?,
    )
    .await
    .context("failed to create redis pool")?;
    config_reload.register_redis_pool("api-service", "REDIS_URL", redis_pool.clone());
    let grpc_rate_limiter = Arc::new(GrpcRateLimiter::new(
        redis_pool,
        std::env::var("GRPC_RATE_LIMIT_RPM")
            .unwrap_or_else(|_| "1000".to_string())
            .parse::<u64>()
            .context("GRPC_RATE_LIMIT_RPM must be a valid u64")?,
    ));

    let provider_repo = Arc::new(PgModelProviderRepository { pool: pool.clone() });
    let model_repo = Arc::new(PgModelCatalogRepository { pool: pool.clone() });
    let pricing_repo = Arc::new(PgModelPricingRepository { pool: pool.clone() });
    let audit_hmac_secret =
        std::env::var("AUDIT_HMAC_SECRET").unwrap_or_else(|_| service_jwt_secret.clone());
    let audit_signer = common::AuditSigner::new(audit_hmac_secret);
    let audit_logger = Arc::new(PgAuditLogger {
        pool: pool.clone(),
        signer: audit_signer,
    });

    let grpc_service = CatalogGrpcService {
        register_provider: RegisterModelProvider {
            repo: provider_repo.clone(),
            audit: audit_logger.clone(),
        },
        register_model: RegisterModel {
            provider_repo: provider_repo.clone(),
            model_repo: model_repo.clone(),
            audit: audit_logger.clone(),
        },
        deprecate_model: DeprecateModel {
            repo: model_repo.clone(),
            audit: audit_logger.clone(),
        },
        set_pricing: SetModelPricing {
            model_repo: model_repo.clone(),
            pricing_repo: pricing_repo.clone(),
            audit: audit_logger.clone(),
        },
        update_capabilities: UpdateModelCapabilities {
            repo: model_repo.clone(),
            audit: audit_logger.clone(),
        },
        upsert_metadata: UpsertModelMetadata {
            model_repo: model_repo.clone(),
            metadata_repo: Arc::new(PgModelMetadataRepository { pool: pool.clone() }),
            audit: audit_logger.clone(),
        },
        set_provider_policy: SetProviderPolicy {
            provider_repo: provider_repo.clone(),
            model_repo: model_repo.clone(),
            policy_repo: Arc::new(PgProviderPolicyRepository { pool: pool.clone() }),
            audit: audit_logger.clone(),
        },
        list_models: ListModels {
            repo: model_repo.clone(),
        },
        get_model: GetModel {
            repo: model_repo.clone(),
        },
        get_by_name: GetModelByPublicName {
            repo: model_repo.clone(),
        },
        get_metadata: GetModelMetadata {
            repo: Arc::new(PgModelMetadataRepository { pool: pool.clone() }),
        },
        get_provider_policy: GetProviderPolicy {
            repo: Arc::new(PgProviderPolicyRepository { pool: pool.clone() }),
        },
        get_pricing: GetCurrentPricing {
            repo: pricing_repo.clone(),
        },
        get_pricing_at: GetPricingAt {
            repo: pricing_repo.clone(),
        },
        list_pricing_history: ListPricingHistory {
            repo: pricing_repo.clone(),
        },
        list_providers: ListProviders {
            repo: provider_repo.clone(),
        },
        admin_callers,
    };

    let mut secrets: Vec<String> = vec![service_jwt_secret];
    if let Ok(prev) = std::env::var("SERVICE_JWT_PREVIOUS_SECRET") {
        if !prev.is_empty() && prev != secrets[0] {
            secrets.push(prev);
        }
    }
    let service_auth = Arc::new(
        ServiceJwtAuth::with_secrets(&secrets, allowed_issuers, allowed_subjects)
            .map_err(|e| anyhow::anyhow!("{e}"))?,
    );
    let auth_interceptor = {
        let service_auth = Arc::clone(&service_auth);
        let rate_limiter = grpc_rate_limiter.clone();
        move |mut req: Request<()>| {
            let token = req
                .metadata()
                .get("authorization")
                .ok_or_else(|| tonic::Status::unauthenticated("missing authorization header"))?
                .to_str()
                .map_err(|_| tonic::Status::unauthenticated("invalid authorization header"))?;
            let token = token
                .strip_prefix("Bearer ")
                .or_else(|| token.strip_prefix("bearer "))
                .ok_or_else(|| tonic::Status::unauthenticated("bearer token required"))?;
            let claims = service_auth.verify(token)?;
            let rate_limiter = rate_limiter.clone();
            let caller = claims.sub.clone();
            tokio::spawn(async move {
                let _ = rate_limiter.check(&caller).await;
            });
            req.extensions_mut().insert(claims);
            Ok(req)
        }
    };

    let addr: std::net::SocketAddr = format!("{bind_addr}:{grpc_port}")
        .parse()
        .with_context(|| format!("invalid bind address: {bind_addr}:{grpc_port}"))?;
    info!("api-service gRPC listening on {}", addr);

    common::tls::validate_server_tls(&service_tls)
        .await
        .context("failed to validate server tls material")?;
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .context("failed to bind gRPC listener")?;
    if service_tls.is_enabled() {
        let incoming = common::tls::dynamic_tls_incoming(listener, service_tls.clone());
        Server::builder()
            .trace_fn(common::observability::grpc_server_span)
            .add_service(CatalogServiceServer::with_interceptor(
                grpc_service,
                auth_interceptor,
            ))
            .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
            .await
            .context("gRPC server failed")?;
    } else {
        let incoming = common::tls::tcp_incoming(listener);
        Server::builder()
            .trace_fn(common::observability::grpc_server_span)
            .add_service(CatalogServiceServer::with_interceptor(
                grpc_service,
                auth_interceptor,
            ))
            .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
            .await
            .context("gRPC server failed")?;
    }

    opentelemetry::global::shutdown_tracer_provider();
    Ok(())
}
