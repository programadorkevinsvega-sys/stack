use anyhow::{Context, Result};
use common::ServiceJwtClaims;
use common::{AppConfig, GrpcRateLimiter, ServiceJwtAuth, ServiceTlsConfig};
use proto::payment_v1::payment_service_client::PaymentServiceClient;
use proto::payment_v1::ChargePayPalVaultRequest;
use proto::plan_resolution_v1::plan_resolution_service_server::PlanResolutionServiceServer;
use proto::plan_resolution_v1::subscription_management_service_server::SubscriptionManagementServiceServer;
use std::collections::HashSet;
use std::sync::Arc;
use subscription_service::application::commands::{
    ChangeSubscription, CreatePlan, CreateSubscription, DeactivatePlan, UpdatePlan,
};
use subscription_service::application::ports::AuditLogger;
use subscription_service::application::queries::{
    GetPlan, GetPlanLimits, GetSubscription, ListActivePlans,
};
use subscription_service::domain::repositories::{
    PlanLimitsCache, PlanRepository, SubscriptionRepository,
};
use subscription_service::infrastructure::cache::RedisPlanLimitsCache;
use subscription_service::infrastructure::persistence::{
    PgAuditLoggerAdapter, PgPlanRepository, PgSubscriptionRepository,
};
use subscription_service::interfaces::grpc::{PlanGrpcService, SubscriptionManagementGrpcService};
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

fn u64_env(key: &str) -> Result<u64> {
    std::env::var(key)
        .with_context(|| format!("{key} is required"))?
        .parse::<u64>()
        .with_context(|| format!("{key} must be a valid u64"))
}

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env_async()
        .await
        .context("failed to load configuration")?;
    let config_reload = common::ConfigReloadHandle::start(config.clone());
    common::observability::init("subscription-service", &config)
        .context("failed to initialize observability")?;
    let service_tls = ServiceTlsConfig::from_env("subscription-service")
        .context("invalid subscription-service TLS configuration")?;
    common::startup::log_service_starting("subscription-service", &config, &service_tls);
    let max_connections = required(config.database_max_connections, "DATABASE_MAX_CONNECTIONS")?;
    let acquire_timeout = required(
        config.database_acquire_timeout_ms,
        "DATABASE_ACQUIRE_TIMEOUT_MS",
    )?;
    let idle_timeout = required(config.database_idle_timeout_ms, "DATABASE_IDLE_TIMEOUT_MS")?;
    let redis_pool_size = required(config.redis_pool_size, "REDIS_POOL_SIZE")?;
    let redis_timeout = required(config.redis_timeout_ms, "REDIS_TIMEOUT_MS")?;
    let cache_ttl = u64_env("SUBSCRIPTION_SERVICE_PLAN_LIMITS_CACHE_TTL_SECONDS")?;
    let bind_addr = std::env::var("SUBSCRIPTION_SERVICE_BIND_ADDR")
        .context("SUBSCRIPTION_SERVICE_BIND_ADDR is required")?;
    let mut secrets: Vec<String> = vec![required(config.service_jwt_secret, "SERVICE_JWT_SECRET")?];
    if let Ok(prev) = std::env::var("SERVICE_JWT_PREVIOUS_SECRET") {
        if !prev.is_empty() && prev != secrets[0] {
            secrets.push(prev);
        }
    }
    let mut allowed_issuers = csv_env("SUBSCRIPTION_SERVICE_ALLOWED_ISSUERS")?;
    let mut allowed_subjects = csv_env("SUBSCRIPTION_SERVICE_ALLOWED_SUBJECTS")?;
    // file-service resolves the active plan before reserving storage. Keep this
    // caller explicit even when the environment contract predates the service.
    allowed_issuers.insert("file-service".into());
    allowed_subjects.insert("file-service".into());
    allowed_issuers.insert("key-services".into());
    allowed_subjects.insert("key-services".into());
    let service_auth = Arc::new(
        ServiceJwtAuth::with_secrets(&secrets, allowed_issuers, allowed_subjects)
            .map_err(|e| anyhow::anyhow!("{e}"))?,
    );
    let admin_callers = csv_env("SUBSCRIPTION_SERVICE_ADMIN_SUBJECTS")?;

    let db_url = std::env::var("SUBSCRIPTION_SERVICE_DATABASE_URL")
        .context("SUBSCRIPTION_SERVICE_DATABASE_URL is required")?;
    let pool = common::startup::connect_postgres(
        "subscription-service",
        &db_url,
        max_connections,
        acquire_timeout,
        idle_timeout,
    )
    .await
    .context("failed to connect postgres")?;
    config_reload.register_postgres_pool(
        "subscription-service",
        "SUBSCRIPTION_SERVICE_DATABASE_URL",
        pool.clone(),
    );

    let redis = common::startup::connect_redis(
        "subscription-service",
        &config.redis_url,
        redis_pool_size,
        redis_timeout,
    )
    .await
    .context("failed to create redis pool")?;
    config_reload.register_redis_pool("subscription-service", "REDIS_URL", redis.clone());
    let cache: Arc<dyn PlanLimitsCache> = Arc::new(RedisPlanLimitsCache {
        pool: redis.clone(),
        ttl: cache_ttl,
    });
    let grpc_rate_limiter = Arc::new(GrpcRateLimiter::new(
        redis.clone(),
        std::env::var("GRPC_RATE_LIMIT_RPM")
            .unwrap_or_else(|_| "1000".to_string())
            .parse::<u64>()
            .context("GRPC_RATE_LIMIT_RPM must be a valid u64")?,
    ));
    let nats = common::connect_nats(&config.nats_url)
        .await
        .context("failed to connect nats")?;
    config_reload.register_nats_client("subscription-service", "NATS_URL", nats.clone());
    let jetstream = async_nats::jetstream::new(nats);
    events::jetstream::ensure_streams(&jetstream)
        .await
        .context("failed to configure JetStream streams")?;
    let relay = events::outbox::OutboxRelay::new(
        pool.clone(),
        jetstream,
        events::outbox::OutboxConfig::for_service("subscription-service", "event_outbox"),
    )
    .context("failed to configure subscription outbox relay")?;
    tokio::spawn(relay.run());
    let lifecycle_pool = pool.clone();
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            ticker.tick().await;
            if let Err(error) = sqlx::query("UPDATE subscriptions SET status='past_due', grace_period_ends_at=COALESCE(grace_period_ends_at, now() + interval '3 days'), payment_retry_count=payment_retry_count+1, last_payment_error='trial_or_renewal_payment_required', updated_at=now() WHERE status='trialing' AND trial_ends_at IS NOT NULL AND trial_ends_at <= now()")
                .execute(&lifecycle_pool).await { tracing::error!(%error, "subscription lifecycle trial transition failed"); }
            if let Err(error) = sqlx::query("UPDATE subscriptions SET status='canceled', canceled_at=COALESCE(canceled_at, now()), updated_at=now() WHERE status='past_due' AND grace_period_ends_at IS NOT NULL AND grace_period_ends_at <= now()")
                .execute(&lifecycle_pool).await { tracing::error!(%error, "subscription lifecycle grace transition failed"); }
        }
    });
    let renewal_pool = pool.clone();
    let renewal_redis = redis.clone();
    let renewal_endpoint = std::env::var("PAYMENT_SERVICES_GRPC_ENDPOINT").ok();
    let renewal_secret = secrets[0].clone();
    let renewal_tls = service_tls.clone();
    tokio::spawn(async move {
        if let Some(endpoint) = renewal_endpoint {
            if let Err(error) = run_paypal_renewal_job(
                renewal_pool,
                renewal_redis,
                endpoint,
                renewal_secret,
                renewal_tls,
            )
            .await
            {
                tracing::error!(%error, "PayPal renewal job stopped");
            }
        } else {
            tracing::error!("PAYMENT_SERVICES_GRPC_ENDPOINT is required for PayPal renewals");
        }
    });
    let plans: Arc<dyn PlanRepository> = Arc::new(PgPlanRepository { pool: pool.clone() });

    let audit_hmac_secret = std::env::var("AUDIT_HMAC_SECRET")
        .or_else(|_| std::env::var("SERVICE_JWT_SECRET"))
        .context("AUDIT_HMAC_SECRET or SERVICE_JWT_SECRET is required")?;
    let audit_logger: Arc<dyn AuditLogger> =
        Arc::new(PgAuditLoggerAdapter::new(common::PgAuditLogger {
            pool: pool.clone(),
            signer: common::AuditSigner::new(audit_hmac_secret),
        }));

    let subscriptions: Arc<dyn SubscriptionRepository> =
        Arc::new(PgSubscriptionRepository { pool });

    let plan_resolution = PlanGrpcService {
        limits: GetPlanLimits {
            subscriptions: subscriptions.clone(),
            plans: plans.clone(),
            cache: cache.clone(),
        },
    };
    let management = SubscriptionManagementGrpcService {
        create_plan: CreatePlan {
            repo: plans.clone(),
            audit_logger: audit_logger.clone(),
        },
        update_plan: UpdatePlan {
            repo: plans.clone(),
            audit_logger: audit_logger.clone(),
            cache: cache.clone(),
        },
        deactivate_plan: DeactivatePlan {
            repo: plans.clone(),
            audit_logger: audit_logger.clone(),
            cache: cache.clone(),
        },
        list_plans: ListActivePlans {
            plans: plans.clone(),
        },
        get_plan: GetPlan {
            plans: plans.clone(),
        },
        create_subscription: CreateSubscription {
            repo: subscriptions.clone(),
            plans: plans.clone(),
            cache: cache.clone(),
            audit_logger: audit_logger.clone(),
        },
        change_subscription: ChangeSubscription {
            repo: subscriptions.clone(),
            plans,
            cache,
            audit_logger: audit_logger.clone(),
        },
        get_subscription: GetSubscription { subscriptions },
        admin_callers,
    };

    let port = required(config.grpc_port, "GRPC_PORT")?;
    let address: std::net::SocketAddr = format!("{bind_addr}:{port}")
        .parse()
        .context("invalid subscription-service bind address")?;
    info!(%address, "subscription-service gRPC listening");

    let resolution_auth = Arc::clone(&service_auth);
    let resolution_rate_limiter = grpc_rate_limiter.clone();
    let resolution_interceptor = move |mut request: Request<()>| {
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
        let claims = resolution_auth.verify(token)?;
        let rate_limiter = resolution_rate_limiter.clone();
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
            .add_service(PlanResolutionServiceServer::with_interceptor(
                plan_resolution,
                resolution_interceptor,
            ))
            .add_service(SubscriptionManagementServiceServer::with_interceptor(
                management,
                management_interceptor,
            ))
            .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
            .await?;
    } else {
        let incoming = common::tls::tcp_incoming(listener);
        Server::builder()
            .trace_fn(common::observability::grpc_server_span)
            .add_service(PlanResolutionServiceServer::with_interceptor(
                plan_resolution,
                resolution_interceptor,
            ))
            .add_service(SubscriptionManagementServiceServer::with_interceptor(
                management,
                management_interceptor,
            ))
            .serve_with_incoming_shutdown(incoming, common::shutdown::shutdown_signal())
            .await?;
    }
    Ok(())
}

async fn run_paypal_renewal_job(
    pool: sqlx::PgPool,
    redis: common::ReloadableRedisPool,
    endpoint: String,
    secret: String,
    tls: ServiceTlsConfig,
) -> Result<(), anyhow::Error> {
    let base = tonic::transport::Endpoint::from_shared(endpoint.clone())?;
    let base = if tls.is_enabled() {
        common::tls::configure_client_endpoint(base, &endpoint, &tls).await?
    } else {
        base
    };
    let channel = loop {
        match base.connect().await {
            Ok(channel) => break channel,
            Err(error) => {
                tracing::warn!(%error, "PayPal payment service unavailable; retrying renewal client connection");
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        }
    };
    let mut ticker = tokio::time::interval(std::time::Duration::from_secs(60));
    loop {
        ticker.tick().await;
        let due = sqlx::query_as::<_, (uuid::Uuid, uuid::Uuid, uuid::Uuid, String)>("SELECT id,user_id,plan_id,billing_cycle FROM subscriptions WHERE status IN ('active','trialing','past_due') AND current_period_end <= now() AND cancel_at_period_end=false AND payment_retry_count < 3 AND (grace_period_ends_at IS NULL OR grace_period_ends_at > now()) FOR UPDATE SKIP LOCKED")
            .fetch_all(&pool).await?;
        for (subscription_id, user_id, plan_id, billing_cycle) in due {
            let Some((amount, currency)) = sqlx::query_as::<_, (rust_decimal::Decimal, String)>(
                "SELECT price_amount,currency FROM plans WHERE id=$1 AND is_active=true",
            )
            .bind(plan_id)
            .fetch_optional(&pool)
            .await?
            else {
                continue;
            };
            let claims = ServiceJwtClaims::new("billing-service", "billing-service", 60)
                .map_err(anyhow::Error::msg)?;
            let token = claims.sign(secret.as_bytes()).map_err(anyhow::Error::msg)?;
            let mut request = Request::new(ChargePayPalVaultRequest {
                user_id: user_id.to_string(),
                amount: amount.to_string(),
                currency,
                idempotency_key: format!(
                    "renewal:{}:{}",
                    subscription_id,
                    chrono::Utc::now().date_naive()
                ),
                reference_id: subscription_id.to_string(),
            });
            request
                .metadata_mut()
                .insert("authorization", format!("Bearer {token}").parse()?);
            let mut client = PaymentServiceClient::new(channel.clone());
            match client.charge_pay_pal_vault(request).await {
                Ok(_) => {
                    let days = if billing_cycle == "annual" { 365 } else { 30 };
                    sqlx::query("UPDATE subscriptions SET status='active',current_period_start=now(),current_period_end=now()+make_interval(days=>$2),payment_retry_count=0,last_payment_error=NULL,grace_period_ends_at=NULL,trial_ends_at=NULL,updated_at=now() WHERE id=$1").bind(subscription_id).bind(days).execute(&pool).await?;
                    let mut connection = redis.get().await?;
                    let _: usize = deadpool_redis::redis::cmd("DEL")
                        .arg(format!("subscription:limits:{user_id}"))
                        .query_async(&mut *connection)
                        .await?;
                }
                Err(error) => {
                    sqlx::query("UPDATE subscriptions SET status='past_due',payment_retry_count=payment_retry_count+1,last_payment_error=$2,current_period_end=now()+interval '1 day',grace_period_ends_at=COALESCE(grace_period_ends_at,now()+interval '3 days'),updated_at=now() WHERE id=$1").bind(subscription_id).bind(error.to_string()).execute(&pool).await?;
                }
            }
        }
    }
}
