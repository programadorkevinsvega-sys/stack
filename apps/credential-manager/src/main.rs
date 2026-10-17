use anyhow::Context;
use aws_config::{BehaviorVersion, Region};
use common::{AppConfig, GrpcRateLimiter, ServiceJwtAuth, ServiceTlsConfig};
use credential_manager::application::commands::{
    MarkRecovered, MarkUnhealthy, UpsertMantleConnection,
};
use credential_manager::application::ports::{
    AuditLogger, KmsCrypto, StsSessionCache, StsTokenProvider,
};
use credential_manager::application::queries::{
    GetAccount, GetCredentials, ListAccounts, ListHealthyAccountsForModel, ListModelsForAccount,
};
use credential_manager::domain::errors::CredentialError;
use credential_manager::infrastructure::cache::redis::RedisStsSessionCache;
use credential_manager::infrastructure::clients::{AwsKmsCrypto, AwsStsTokenProvider};
use credential_manager::infrastructure::persistence::{
    PgAuditLoggerAdapter, PgAwsAccountModelRepository, PgAwsCredentialRepository,
};
use credential_manager::interfaces::grpc::CredentialGrpcService;
use proto::credential_resolution_v1::credential_resolution_service_server::CredentialResolutionServiceServer;
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
use tonic::transport::Server;
use tonic::Request;
use tracing::{error, info, warn};

fn parse_csv_env(key: &str) -> Result<HashSet<String>, CredentialError> {
    let value = std::env::var(key)
        .map_err(|_| CredentialError::Configuration(format!("{key} is required")))?;
    let parsed: HashSet<String> = value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(ToOwned::to_owned)
        .collect();
    if parsed.is_empty() {
        return Err(CredentialError::Configuration(format!(
            "{key} must contain at least one value"
        )));
    }
    Ok(parsed)
}

fn parse_i32_env_required(key: &str) -> Result<i32, CredentialError> {
    let value = std::env::var(key)
        .map_err(|_| CredentialError::Configuration(format!("{key} is required")))?;
    value
        .parse::<i32>()
        .map_err(|e| CredentialError::Configuration(format!("{key} must be a valid integer: {e}")))
}

fn require_option<T>(value: Option<T>, name: &str) -> Result<T, CredentialError> {
    value.ok_or_else(|| CredentialError::Configuration(format!("{name} is required")))
}

async fn ensure_required_relations(pool: &PgPool) -> Result<(), CredentialError> {
    let required_relations = ["aws_credential_pool", "aws_account_models"];

    for relation in required_relations {
        let exists = sqlx::query_scalar::<_, Option<String>>("SELECT to_regclass($1)::text")
            .bind(relation)
            .fetch_one(pool)
            .await
            .map_err(|e| {
                CredentialError::DependencyUnavailable(format!(
                    "failed checking relation existence {relation}: {e}"
                ))
            })?;

        if exists.is_none() {
            return Err(CredentialError::Configuration(format!(
                "required relation not found: {relation}"
            )));
        }
    }

    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = AppConfig::from_env_async().await?;
    let config_reload = common::ConfigReloadHandle::start(config.clone());
    let service_tls = ServiceTlsConfig::from_env("credential-manager")
        .context("invalid credential-manager TLS configuration")?;
    common::observability::init("credential-manager", &config)?;
    common::startup::log_service_starting("credential-manager", &config, &service_tls);

    let jwt_secret = config
        .service_jwt_secret
        .clone()
        .ok_or_else(|| CredentialError::Configuration("SERVICE_JWT_SECRET is required".into()))?;
    let allowed_issuers = parse_csv_env("CREDENTIAL_MANAGER_ALLOWED_ISSUERS")?;
    let allowed_subjects = parse_csv_env("CREDENTIAL_MANAGER_ALLOWED_SUBJECTS")?;
    let sts_session_duration_seconds = parse_i32_env_required("STS_SESSION_DURATION_SECONDS")?;
    let sts_refresh_buffer_seconds = parse_i32_env_required("STS_REFRESH_BUFFER_SECONDS")? as i64;
    let static_keys_cache_ttl_seconds =
        parse_i32_env_required("STATIC_KEYS_CACHE_TTL_SECONDS")? as i64;
    let health_check_interval_seconds =
        parse_i32_env_required("CREDENTIAL_MANAGER_HEALTH_CHECK_INTERVAL_SECONDS")?;
    let bind_addr = std::env::var("CREDENTIAL_MANAGER_BIND_ADDR").map_err(|_| {
        CredentialError::Configuration("CREDENTIAL_MANAGER_BIND_ADDR is required".into())
    })?;
    let aws_region = require_option(config.aws_region.clone(), "AWS_REGION")?;

    let db_max_connections =
        require_option(config.database_max_connections, "DATABASE_MAX_CONNECTIONS")?;
    let db_acquire_timeout_ms = require_option(
        config.database_acquire_timeout_ms,
        "DATABASE_ACQUIRE_TIMEOUT_MS",
    )?;
    let db_idle_timeout_ms =
        require_option(config.database_idle_timeout_ms, "DATABASE_IDLE_TIMEOUT_MS")?;
    let grpc_port = require_option(config.grpc_port, "GRPC_PORT")?;
    if !(900..=43200).contains(&sts_session_duration_seconds) {
        return Err(CredentialError::Configuration(
            "STS_SESSION_DURATION_SECONDS must be between 900 and 43200".to_string(),
        )
        .into());
    }
    if !(30..=3600).contains(&sts_refresh_buffer_seconds) {
        return Err(CredentialError::Configuration(
            "STS_REFRESH_BUFFER_SECONDS must be between 30 and 3600".to_string(),
        )
        .into());
    }
    if sts_refresh_buffer_seconds >= i64::from(sts_session_duration_seconds) {
        return Err(CredentialError::Configuration(
            "STS_REFRESH_BUFFER_SECONDS must be lower than STS_SESSION_DURATION_SECONDS"
                .to_string(),
        )
        .into());
    }
    if !(60..=86_400).contains(&static_keys_cache_ttl_seconds) {
        return Err(CredentialError::Configuration(
            "STATIC_KEYS_CACHE_TTL_SECONDS must be between 60 and 86400".to_string(),
        )
        .into());
    }
    if !(10..=3600).contains(&health_check_interval_seconds) {
        return Err(CredentialError::Configuration(
            "CREDENTIAL_MANAGER_HEALTH_CHECK_INTERVAL_SECONDS must be between 10 and 3600"
                .to_string(),
        )
        .into());
    }

    let db_url = std::env::var("CREDENTIAL_MANAGER_DATABASE_URL")
        .context("CREDENTIAL_MANAGER_DATABASE_URL is required")?;
    let pool = common::startup::connect_postgres(
        "credential-manager",
        &db_url,
        db_max_connections,
        db_acquire_timeout_ms,
        db_idle_timeout_ms,
    )
    .await?;
    config_reload.register_postgres_pool(
        "credential-manager",
        "CREDENTIAL_MANAGER_DATABASE_URL",
        pool.clone(),
    );
    ensure_required_relations(&pool).await?;
    info!("Required database relations verified");

    let nats = common::connect_nats(&config.nats_url).await?;
    config_reload.register_nats_client("credential-manager", "NATS_URL", nats.clone());
    let jetstream = async_nats::jetstream::new(nats);
    events::jetstream::ensure_streams(&jetstream)
        .await
        .map_err(|error| anyhow::anyhow!(error))?;
    let relay = events::outbox::OutboxRelay::new(
        pool.clone(),
        jetstream,
        events::outbox::OutboxConfig::for_service("credential-manager", "event_outbox"),
    )
    .map_err(|error| anyhow::anyhow!(error))?;
    tokio::spawn(relay.run());
    info!("NATS JetStream and outbox relay ready");

    let redis_pool = common::startup::connect_redis(
        "credential-manager",
        &config.redis_url,
        require_option(config.redis_pool_size, "REDIS_POOL_SIZE")?,
        require_option(config.redis_timeout_ms, "REDIS_TIMEOUT_MS")?,
    )
    .await
    .context("failed to create redis pool")?;
    config_reload.register_redis_pool("credential-manager", "REDIS_URL", redis_pool.clone());
    let grpc_rate_limiter = Arc::new(GrpcRateLimiter::new(
        redis_pool.clone(),
        std::env::var("GRPC_RATE_LIMIT_RPM")
            .unwrap_or_else(|_| "1000".to_string())
            .parse::<u64>()
            .context("GRPC_RATE_LIMIT_RPM must be a valid u64")?,
    ));

    let mut aws_loader = aws_config::defaults(BehaviorVersion::latest());
    aws_loader = aws_loader.region(Region::new(aws_region));
    if let Some(endpoint) = config.aws_endpoint_url.clone() {
        aws_loader = aws_loader.endpoint_url(endpoint);
    }
    let aws_shared_config = aws_loader.load().await;
    let kms_client = aws_sdk_kms::Client::new(&aws_shared_config);
    let sts_client = aws_sdk_sts::Client::new(&aws_shared_config);

    let kms_crypto: Arc<dyn KmsCrypto> = Arc::new(AwsKmsCrypto::new(kms_client));
    let mantle_api_kms_key_id = std::env::var("MANTLE_API_KMS_KEY_ID")
        .map_err(|_| CredentialError::Configuration("MANTLE_API_KMS_KEY_ID is required".into()))?;
    let mantle_role_arn = std::env::var("MANTLE_ROLE_ARN").ok();
    let sts_cache: Arc<dyn StsSessionCache> = Arc::new(RedisStsSessionCache::new(
        redis_pool.clone(),
        sts_session_duration_seconds,
        sts_refresh_buffer_seconds,
    ));
    let sts_provider: Arc<dyn StsTokenProvider> = Arc::new(AwsStsTokenProvider::new(
        aws_shared_config.clone(),
        sts_client,
        kms_crypto.clone(),
        sts_cache,
        sts_session_duration_seconds,
        static_keys_cache_ttl_seconds,
        mantle_role_arn,
    )?);

    let audit_hmac_secret = std::env::var("AUDIT_HMAC_SECRET")
        .or_else(|_| std::env::var("SERVICE_JWT_SECRET"))
        .map_err(|_| {
            CredentialError::Configuration(
                "AUDIT_HMAC_SECRET or SERVICE_JWT_SECRET is required".into(),
            )
        })?;
    let audit_logger: Arc<dyn AuditLogger> =
        Arc::new(PgAuditLoggerAdapter::new(common::PgAuditLogger {
            pool: pool.clone(),
            signer: common::AuditSigner::new(audit_hmac_secret),
        }));

    let cred_repo = Arc::new(PgAwsCredentialRepository { pool: pool.clone() });

    let get_credentials = GetCredentials {
        repo: cred_repo.clone(),
        sts_provider: sts_provider.clone(),
    };
    let get_account = GetAccount {
        repo: cred_repo.clone(),
    };
    let list_models_for_account = ListModelsForAccount {
        repo: Arc::new(PgAwsAccountModelRepository { pool: pool.clone() }),
    };
    let account_model_repo = Arc::new(PgAwsAccountModelRepository { pool: pool.clone() });
    let get_mantle_connection = credential_manager::application::queries::GetMantleConnection {
        repo: account_model_repo.clone(),
        kms: kms_crypto.clone(),
    };

    let grpc_service = CredentialGrpcService {
        get_credentials,
        get_account,
        list_models_for_account,
        list_healthy_accounts_for_model: ListHealthyAccountsForModel { repo: cred_repo },
        list_accounts: ListAccounts {
            repo: Arc::new(PgAwsCredentialRepository { pool: pool.clone() }),
        },
        get_mantle_connection,
        upsert_mantle_connection: UpsertMantleConnection {
            repo: account_model_repo,
            kms: kms_crypto,
            audit_logger: audit_logger.clone(),
            kms_key_id: mantle_api_kms_key_id,
        },
    };

    let list_accounts = ListAccounts {
        repo: Arc::new(PgAwsCredentialRepository { pool: pool.clone() }),
    };
    let health_repo = Arc::new(PgAwsCredentialRepository { pool: pool.clone() });
    let mark_unhealthy = MarkUnhealthy {
        repo: health_repo.clone(),
        sts_provider: sts_provider.clone(),
        audit_logger: audit_logger.clone(),
    };
    let mark_recovered = MarkRecovered {
        repo: health_repo,
        sts_provider: sts_provider.clone(),
        audit_logger: audit_logger.clone(),
    };

    let health_check_duration = Duration::from_secs(health_check_interval_seconds as u64);
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(health_check_duration);
        loop {
            ticker.tick().await;
            let accounts = match list_accounts.execute().await {
                Ok(accounts) => accounts,
                Err(e) => {
                    error!(error = %e, "Health check loop failed to list accounts");
                    continue;
                }
            };

            for account in accounts {
                let probe = sts_provider.get_session_token(&account).await;
                match probe {
                    Ok(_) => {
                        if !account.is_healthy {
                            if let Err(e) = mark_recovered.execute(account.id).await {
                                error!(account_id = %account.id, error = %e, "Failed to mark account recovered");
                            }
                        }
                    }
                    Err(probe_err) => {
                        warn!(
                            account_id = %account.id,
                            aws_account_id = %account.aws_account_id,
                            error = %probe_err,
                            "Account health probe failed"
                        );
                        if let Err(update_err) = mark_unhealthy
                            .execute(account.id, "health_check_failed".to_string())
                            .await
                        {
                            error!(
                                account_id = %account.id,
                                error = %update_err,
                                "Failed to mark account unhealthy"
                            );
                        }
                    }
                }
            }
        }
    });

    let mut secrets: Vec<String> = vec![jwt_secret];
    if let Ok(prev) = std::env::var("SERVICE_JWT_PREVIOUS_SECRET") {
        if !prev.is_empty() && prev != secrets[0] {
            secrets.push(prev);
        }
    }
    let service_auth = Arc::new(
        ServiceJwtAuth::with_secrets(&secrets, allowed_issuers, allowed_subjects)
            .map_err(CredentialError::Configuration)?,
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

    let addr: std::net::SocketAddr = format!("{bind_addr}:{grpc_port}").parse().map_err(|e| {
        CredentialError::Configuration(format!("invalid bind address {bind_addr}:{grpc_port}: {e}"))
    })?;
    info!("credential-manager gRPC listening on {addr}");

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
            .add_service(CredentialResolutionServiceServer::with_interceptor(
                grpc_service,
                auth_interceptor,
            ))
            .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
            .await?;
    } else {
        let incoming = common::tls::tcp_incoming(listener);
        Server::builder()
            .trace_fn(common::observability::grpc_server_span)
            .add_service(CredentialResolutionServiceServer::with_interceptor(
                grpc_service,
                auth_interceptor,
            ))
            .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
            .await?;
    }

    opentelemetry::global::shutdown_tracer_provider();
    Ok(())
}
