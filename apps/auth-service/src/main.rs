use anyhow::{Context, Result};
use auth_service::application::commands::{
    ChangePassword, DeleteUser, DisableUser, Login, Logout, LogoutAll, ManageMfa,
    RefreshAccessToken, RegisterUser, RequestEmailVerification, RequestPasswordReset,
    ResetPassword, UnlockAccount, UpdateProfile, VerifyEmail,
};
use auth_service::application::ports::{
    AuditLogger, LoginRateLimiter, PasswordHasher, SessionStore,
};
use auth_service::application::queries::{GetUser, GetUserByEmail, ListUsers, ValidateAccessToken};
use auth_service::domain::repositories::{RefreshTokenRepository, UserRepository};
use auth_service::infrastructure::crypto::{
    Argon2Hasher, AwsKmsSensitiveDataCrypto, JwtSigner, ResendEmailSender, TotpRsService,
};
use auth_service::infrastructure::persistence::{
    PgAuditLogger, PgEmailVerificationRepository, PgPasswordResetRepository,
    PgRefreshTokenRepository, PgUserRepository,
};
use auth_service::infrastructure::rate_limit::RedisLoginRateLimiter;
use auth_service::infrastructure::session::RedisSessionStore;
use auth_service::interfaces::grpc::AuthGrpcService;
use aws_config::{BehaviorVersion, Region};
use aws_sdk_secretsmanager::Client as SecretsManagerClient;
use chrono::Utc;
use common::{AppConfig, GrpcRateLimiter, ServiceJwtAuth, ServiceTlsConfig};
use proto::auth_v1::auth_service_server::AuthServiceServer;
use serde::Deserialize;
use sqlx::PgPool;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;
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

fn parse_i64_env(key: &str) -> Result<i64> {
    let value = std::env::var(key).with_context(|| format!("{key} is required"))?;
    value
        .parse::<i64>()
        .with_context(|| format!("{key} must be a valid integer"))
}

fn parse_u32_env(key: &str) -> Result<u32> {
    let value = std::env::var(key).with_context(|| format!("{key} is required"))?;
    value
        .parse::<u32>()
        .with_context(|| format!("{key} must be a valid integer"))
}

fn require_option<T>(value: Option<T>, name: &str) -> Result<T> {
    value.with_context(|| format!("{name} is required"))
}

async fn ensure_required_relations(pool: &PgPool) -> Result<()> {
    let required_relations = [
        "users",
        "refresh_tokens",
        "password_resets",
        "email_verifications",
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

#[derive(Debug, Deserialize)]
struct AdminCredentialSecret {
    email: String,
    password: String,
}

async fn load_admin_credentials(
    client: &SecretsManagerClient,
    secret_id: &str,
) -> Result<AdminCredentialSecret> {
    let response = client
        .get_secret_value()
        .secret_id(secret_id)
        .send()
        .await
        .with_context(|| format!("failed to load admin credentials from AWS secret {secret_id}"))?;
    let secret_string = response
        .secret_string()
        .context("admin credentials secret must contain SecretString")?;
    let credentials: AdminCredentialSecret = serde_json::from_str(secret_string)
        .context("admin credentials secret must be valid JSON with email and password")?;
    let email = credentials.email.trim().to_ascii_lowercase();
    if email != "programadorkevinsvega@gmail.com" {
        anyhow::bail!("admin credentials secret email is not the configured administrator");
    }
    if credentials.password.chars().count() < 16 {
        anyhow::bail!("admin credentials secret password must contain at least 16 characters");
    }
    Ok(AdminCredentialSecret {
        email,
        password: credentials.password,
    })
}

async fn synchronize_admin_credentials(
    client: &SecretsManagerClient,
    secret_id: &str,
    user_repo: &dyn UserRepository,
    refresh_repo: &dyn RefreshTokenRepository,
    sessions: &dyn SessionStore,
    hasher: &dyn PasswordHasher,
) -> Result<()> {
    let credentials = load_admin_credentials(client, secret_id).await?;
    let existing = user_repo
        .find_by_email(&credentials.email)
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))?;

    match existing {
        Some(user) => {
            let password_matches = hasher
                .verify(&user.password_hash, &credentials.password)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let role_needs_update = user.role != "admin";
            if !password_matches || role_needs_update || user.email_verified_at.is_none() {
                let mut updated = user.clone();
                updated.role = "admin".to_string();
                updated.email_verified_at = Some(Utc::now());
                if !password_matches {
                    updated.password_hash = hasher
                        .hash(&credentials.password)
                        .await
                        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                }
                user_repo
                    .update(&updated)
                    .await
                    .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                if !password_matches {
                    refresh_repo
                        .revoke_all_for_user(user.id)
                        .await
                        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                    sessions
                        .revoke_all_for_user(user.id)
                        .await
                        .map_err(|error| anyhow::anyhow!(error.to_string()))?;
                    tracing::info!(user_id = %user.id, "admin password rotated from AWS secret");
                }
            }
            tracing::debug!(user_id = %user.id, "admin credentials synchronized");
        }
        None => {
            let password_hash = hasher
                .hash(&credentials.password)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            let mut admin = auth_service::domain::entities::User::new(
                credentials.email,
                password_hash,
                Some("Nexus Administrator".to_string()),
            )
            .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            admin.role = "admin".to_string();
            admin.email_verified_at = Some(Utc::now());
            let admin_id = admin.id;
            user_repo
                .save(&admin)
                .await
                .map_err(|error| anyhow::anyhow!(error.to_string()))?;
            tracing::info!(user_id = %admin_id, "admin account provisioned from AWS secret");
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
    common::observability::init("auth-service", &config)
        .context("failed to initialize observability")?;
    let service_tls = ServiceTlsConfig::from_env("auth-service")
        .context("invalid auth-service TLS configuration")?;
    common::startup::log_service_starting("auth-service", &config, &service_tls);

    let service_jwt_secret =
        require_option(config.service_jwt_secret.clone(), "SERVICE_JWT_SECRET")?;
    let jwt_signing_secret =
        require_option(config.jwt_signing_secret.clone(), "JWT_SIGNING_SECRET")?;

    let allowed_issuers = parse_csv_env("AUTH_SERVICE_ALLOWED_ISSUERS")?;
    let allowed_subjects = parse_csv_env("AUTH_SERVICE_ALLOWED_SUBJECTS")?;
    let admin_callers = parse_csv_env("AUTH_SERVICE_ADMIN_SUBJECTS")?;
    if !admin_callers.is_subset(&allowed_subjects) {
        anyhow::bail!(
            "AUTH_SERVICE_ADMIN_SUBJECTS must be a subset of AUTH_SERVICE_ALLOWED_SUBJECTS"
        );
    }
    let bind_addr =
        std::env::var("AUTH_SERVICE_BIND_ADDR").context("AUTH_SERVICE_BIND_ADDR is required")?;

    let resend_api_key = std::env::var("RESEND_API_KEY").context("RESEND_API_KEY is required")?;
    let resend_from_email =
        std::env::var("RESEND_FROM_EMAIL").context("RESEND_FROM_EMAIL is required")?;
    let auth_app_base_url =
        std::env::var("AUTH_APP_BASE_URL").context("AUTH_APP_BASE_URL is required")?;

    let auth_mfa_issuer =
        std::env::var("AUTH_MFA_ISSUER").context("AUTH_MFA_ISSUER is required")?;
    let auth_mfa_kms_key_id =
        std::env::var("AUTH_MFA_KMS_KEY_ID").context("AUTH_MFA_KMS_KEY_ID is required")?;
    let email_verification_ttl_minutes = parse_i64_env("AUTH_EMAIL_VERIFICATION_TTL_MINUTES")?;
    if !(5..=1440).contains(&email_verification_ttl_minutes) {
        anyhow::bail!("AUTH_EMAIL_VERIFICATION_TTL_MINUTES must be between 5 and 1440");
    }
    let login_rate_limit_max_attempts = parse_u32_env("AUTH_LOGIN_RATE_LIMIT_MAX_ATTEMPTS")?;
    let login_rate_limit_window_seconds = parse_u32_env("AUTH_LOGIN_RATE_LIMIT_WINDOW_SECONDS")?;
    let session_ttl_seconds = parse_i64_env("AUTH_SESSION_TTL_SECONDS")?;
    if !(300..=2_592_000).contains(&session_ttl_seconds) {
        anyhow::bail!("AUTH_SESSION_TTL_SECONDS must be between 300 and 2592000");
    }
    if !(3..=20).contains(&login_rate_limit_max_attempts) {
        anyhow::bail!("AUTH_LOGIN_RATE_LIMIT_MAX_ATTEMPTS must be between 3 and 20");
    }
    if !(60..=3600).contains(&login_rate_limit_window_seconds) {
        anyhow::bail!("AUTH_LOGIN_RATE_LIMIT_WINDOW_SECONDS must be between 60 and 3600");
    }

    let db_max_connections =
        require_option(config.database_max_connections, "DATABASE_MAX_CONNECTIONS")?;
    let db_acquire_timeout_ms = require_option(
        config.database_acquire_timeout_ms,
        "DATABASE_ACQUIRE_TIMEOUT_MS",
    )?;
    let db_idle_timeout_ms =
        require_option(config.database_idle_timeout_ms, "DATABASE_IDLE_TIMEOUT_MS")?;
    let redis_pool_size = require_option(config.redis_pool_size, "REDIS_POOL_SIZE")?;
    let redis_timeout_ms = require_option(config.redis_timeout_ms, "REDIS_TIMEOUT_MS")?;
    let grpc_port = require_option(config.grpc_port, "GRPC_PORT")?;

    let db_url = std::env::var("AUTH_SERVICE_DATABASE_URL")
        .context("AUTH_SERVICE_DATABASE_URL is required")?;
    let pool = common::startup::connect_postgres(
        "auth-service",
        &db_url,
        db_max_connections,
        db_acquire_timeout_ms,
        db_idle_timeout_ms,
    )
    .await
    .context("failed to connect postgres")?;
    config_reload.register_postgres_pool("auth-service", "AUTH_SERVICE_DATABASE_URL", pool.clone());

    ensure_required_relations(&pool).await?;
    info!("Required database relations verified");

    let redis_pool = common::startup::connect_redis(
        "auth-service",
        &config.redis_url,
        redis_pool_size,
        redis_timeout_ms,
    )
    .await
    .context("failed to create redis pool for auth rate-limit")?;
    config_reload.register_redis_pool("auth-service", "REDIS_URL", redis_pool.clone());

    let aws_region = require_option(config.aws_region.clone(), "AWS_REGION")?;
    let mut aws_loader = aws_config::defaults(BehaviorVersion::latest());
    aws_loader = aws_loader.region(Region::new(aws_region.clone()));
    if let Some(endpoint) = config.aws_endpoint_url.clone() {
        aws_loader = aws_loader.endpoint_url(endpoint);
    }
    let aws_shared_config = aws_loader.load().await;
    let kms_client = aws_sdk_kms::Client::new(&aws_shared_config);
    let secrets_client = SecretsManagerClient::new(&aws_shared_config);
    info!(
        dependency = "aws_kms",
        region = %aws_region,
        endpoint = %config
            .aws_endpoint_url
            .as_deref()
            .map(common::startup::sanitize_endpoint)
            .unwrap_or_else(|| "<aws-default>".to_string()),
        status = "configured",
        "dependency client configured; connectivity is checked on first operation"
    );

    let user_repo = Arc::new(PgUserRepository { pool: pool.clone() });
    let refresh_repo = Arc::new(PgRefreshTokenRepository { pool: pool.clone() });
    let reset_repo = Arc::new(PgPasswordResetRepository { pool: pool.clone() });
    let verification_repo = Arc::new(PgEmailVerificationRepository { pool: pool.clone() });
    let audit_hmac_secret =
        std::env::var("AUDIT_HMAC_SECRET").unwrap_or_else(|_| service_jwt_secret.clone());
    let audit_signer = common::AuditSigner::new(audit_hmac_secret);
    let audit: Arc<dyn AuditLogger> = Arc::new(PgAuditLogger {
        pool: pool.clone(),
        signer: audit_signer,
    });
    let grpc_rate_limiter = Arc::new(GrpcRateLimiter::new(
        redis_pool.clone(),
        std::env::var("GRPC_RATE_LIMIT_RPM")
            .unwrap_or_else(|_| "1000".to_string())
            .parse::<u64>()
            .context("GRPC_RATE_LIMIT_RPM must be a valid u64")?,
    ));
    let login_limiter: Arc<dyn LoginRateLimiter> = Arc::new(RedisLoginRateLimiter::new(
        redis_pool.clone(),
        login_rate_limit_max_attempts,
        login_rate_limit_window_seconds,
    )?);
    let sessions: Arc<dyn SessionStore> = Arc::new(RedisSessionStore::new(redis_pool));

    let hasher = Arc::new(Argon2Hasher);
    let admin_secret_id = std::env::var("AUTH_ADMIN_CREDENTIALS_SECRET_ID")
        .context("AUTH_ADMIN_CREDENTIALS_SECRET_ID is required")?;
    if admin_secret_id.trim().is_empty() {
        anyhow::bail!("AUTH_ADMIN_CREDENTIALS_SECRET_ID must not be empty");
    }
    let admin_refresh_seconds = std::env::var("AUTH_ADMIN_CREDENTIALS_REFRESH_SECONDS")
        .context("AUTH_ADMIN_CREDENTIALS_REFRESH_SECONDS is required")?
        .parse::<u64>()
        .context("AUTH_ADMIN_CREDENTIALS_REFRESH_SECONDS must be a valid integer")?;
    if !(15..=86_400).contains(&admin_refresh_seconds) {
        anyhow::bail!(
            "AUTH_ADMIN_CREDENTIALS_REFRESH_SECONDS must be between 15 and 86400 seconds"
        );
    }
    let user_repository: Arc<dyn UserRepository> = user_repo.clone();
    let refresh_repository: Arc<dyn RefreshTokenRepository> = refresh_repo.clone();
    synchronize_admin_credentials(
        &secrets_client,
        &admin_secret_id,
        user_repository.as_ref(),
        refresh_repository.as_ref(),
        sessions.as_ref(),
        hasher.as_ref(),
    )
    .await
    .context("failed to synchronize admin credentials from AWS")?;
    let sync_client = secrets_client.clone();
    let sync_secret_id = admin_secret_id.clone();
    let sync_user_repository = user_repository.clone();
    let sync_refresh_repository = refresh_repository.clone();
    let sync_sessions = sessions.clone();
    let sync_hasher = hasher.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(admin_refresh_seconds));
        interval.tick().await;
        loop {
            interval.tick().await;
            if let Err(error) = synchronize_admin_credentials(
                &sync_client,
                &sync_secret_id,
                sync_user_repository.as_ref(),
                sync_refresh_repository.as_ref(),
                sync_sessions.as_ref(),
                sync_hasher.as_ref(),
            )
            .await
            {
                tracing::error!(error = %error, "admin credentials synchronization failed");
            }
        }
    });
    let signer = Arc::new(JwtSigner::new(jwt_signing_secret));
    let email_sender = Arc::new(ResendEmailSender::new(
        resend_api_key,
        resend_from_email,
        auth_app_base_url,
    )?);
    let secret_crypto = Arc::new(AwsKmsSensitiveDataCrypto::new(
        kms_client,
        auth_mfa_kms_key_id,
    )?);
    let totp = Arc::new(TotpRsService::new(auth_mfa_issuer)?);

    let grpc_service = AuthGrpcService {
        register: RegisterUser {
            repo: user_repo.clone(),
            hasher: hasher.clone(),
            email: email_sender.clone(),
            verification_repo: verification_repo.clone(),
            email_verification_ttl_minutes,
        },
        verify_email: VerifyEmail {
            user_repo: user_repo.clone(),
            verification_repo: verification_repo.clone(),
            audit: audit.clone(),
        },
        request_email_verification: RequestEmailVerification {
            user_repo: user_repo.clone(),
            verification_repo: verification_repo.clone(),
            email: email_sender.clone(),
            email_verification_ttl_minutes,
        },
        login: Login {
            repo: user_repo.clone(),
            hasher: hasher.clone(),
            signer: signer.clone(),
            refresh_repo: refresh_repo.clone(),
            sessions: sessions.clone(),
            session_ttl_seconds: session_ttl_seconds as u64,
            secret_crypto: secret_crypto.clone(),
            totp: totp.clone(),
            login_limiter: login_limiter.clone(),
            audit: audit.clone(),
        },
        refresh: RefreshAccessToken {
            refresh_repo: refresh_repo.clone(),
            signer: signer.clone(),
            user_repo: user_repo.clone(),
            sessions: sessions.clone(),
            session_ttl_seconds: session_ttl_seconds as u64,
        },
        logout: Logout {
            refresh_repo: refresh_repo.clone(),
            sessions: sessions.clone(),
        },
        logout_all: LogoutAll {
            refresh_repo: refresh_repo.clone(),
            sessions: sessions.clone(),
        },
        request_reset: RequestPasswordReset {
            repo: user_repo.clone(),
            reset_repo: reset_repo.clone(),
            email: email_sender.clone(),
        },
        reset_password: ResetPassword {
            reset_repo: reset_repo.clone(),
            user_repo: user_repo.clone(),
            hasher: hasher.clone(),
            refresh_repo: refresh_repo.clone(),
        },
        change_password: ChangePassword {
            repo: user_repo.clone(),
            hasher: hasher.clone(),
            refresh_repo: refresh_repo.clone(),
        },
        manage_mfa: ManageMfa {
            repo: user_repo.clone(),
            secret_crypto: secret_crypto.clone(),
            totp: totp.clone(),
            refresh_repo: refresh_repo.clone(),
            audit: audit.clone(),
        },
        update_profile: UpdateProfile {
            repo: user_repo.clone(),
        },
        unlock_account: UnlockAccount {
            repo: user_repo.clone(),
            audit: audit.clone(),
        },
        disable_user: DisableUser {
            repo: user_repo.clone(),
            refresh_repo: refresh_repo.clone(),
            audit: audit.clone(),
        },
        delete_user: DeleteUser {
            repo: user_repo.clone(),
            refresh_repo: refresh_repo.clone(),
            audit: audit.clone(),
        },
        get_user: GetUser {
            repo: user_repo.clone(),
        },
        get_user_by_email: GetUserByEmail {
            repo: user_repo.clone(),
        },
        validate_access_token: ValidateAccessToken {
            repo: user_repo.clone(),
            signer: signer.clone(),
            sessions: sessions.clone(),
        },
        sessions: sessions.clone(),
        list_users: ListUsers { repo: user_repo },
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
    info!("auth-service gRPC listening on {}", addr);

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
            .add_service(AuthServiceServer::with_interceptor(
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
            .add_service(AuthServiceServer::with_interceptor(
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
