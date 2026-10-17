use anyhow::{Context, Result};
use api_gateway::application::service::{ApiGatewayApplication, GatewayConfig};
use api_gateway::infrastructure::clients::{
    BillingClient, CatalogClient, CredentialClient, KeyClient, ModelRouterClient, PlanClient,
    ServiceClientConfig,
};
use api_gateway::infrastructure::events::GatewayEventPublisher;
use api_gateway::infrastructure::key_cache::CachedKeyClient;
use api_gateway::infrastructure::rate_limit::RedisRateLimiter;
use api_gateway::interfaces::http::{router, HttpState};
use common::{AppConfig, ServiceTlsConfig};
use futures_util::StreamExt;
use metrics_exporter_prometheus::PrometheusBuilder;
use rust_decimal::Decimal;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;
use tracing::Instrument;

fn required<T>(value: Option<T>, name: &str) -> Result<T> {
    value.with_context(|| format!("{name} is required"))
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

#[tokio::main]
async fn main() -> Result<()> {
    let config = AppConfig::from_env_async()
        .await
        .context("failed to load configuration")?;
    let config_reload = common::ConfigReloadHandle::start(config.clone());
    common::observability::init("api-gateway", &config)
        .context("failed to initialize observability")?;
    let gateway_database_url =
        std::env::var("API_GATEWAY_DATABASE_URL").unwrap_or_else(|_| config.database_url.clone());
    let pool = common::startup::connect_postgres(
        "api-gateway",
        &gateway_database_url,
        required(config.database_max_connections, "DATABASE_MAX_CONNECTIONS")?,
        required(
            config.database_acquire_timeout_ms,
            "DATABASE_ACQUIRE_TIMEOUT_MS",
        )?,
        required(config.database_idle_timeout_ms, "DATABASE_IDLE_TIMEOUT_MS")?,
    )
    .await
    .context("failed to connect api-gateway outbox database")?;
    config_reload.register_postgres_pool("api-gateway", "API_GATEWAY_DATABASE_URL", pool.clone());
    let jwt_secret = required(config.service_jwt_secret.clone(), "SERVICE_JWT_SECRET")?;
    let issuer = string_env("API_GATEWAY_SERVICE_JWT_ISSUER")?;
    let subject = string_env("API_GATEWAY_SERVICE_JWT_SUBJECT")?;
    let service_tls = ServiceTlsConfig::from_env("api-gateway")
        .context("invalid api-gateway TLS configuration")?;
    common::startup::log_service_starting("api-gateway", &config, &service_tls);
    let tls_config = service_tls.is_enabled().then(|| service_tls.clone());
    let client_config = |endpoint_key: &str| -> Result<ServiceClientConfig> {
        Ok(ServiceClientConfig {
            endpoint: string_env(endpoint_key)?,
            jwt_secret: jwt_secret.clone(),
            issuer: issuer.clone(),
            subject: subject.clone(),
            tls_config: tls_config.clone(),
        })
    };

    let redis = common::startup::connect_redis(
        "api-gateway",
        &config.redis_url,
        required(config.redis_pool_size, "REDIS_POOL_SIZE")?,
        required(config.redis_timeout_ms, "REDIS_TIMEOUT_MS")?,
    )
    .await
    .context("failed to create redis pool")?;
    config_reload.register_redis_pool("api-gateway", "REDIS_URL", redis.clone());
    let nats = common::connect_nats(&config.nats_url)
        .await
        .context("failed to connect nats")?;
    config_reload.register_nats_client("api-gateway", "NATS_URL", nats.clone());
    let jetstream = async_nats::jetstream::new(nats.clone());
    events::jetstream::ensure_streams(&jetstream)
        .await
        .context("failed to configure JetStream streams")?;
    let outbox = events::outbox::OutboxStore::new(pool.clone(), "event_outbox")
        .context("failed to configure api-gateway outbox")?;
    let relay = events::outbox::OutboxRelay::new(
        pool,
        jetstream.clone(),
        events::outbox::OutboxConfig::for_service("api-gateway", "event_outbox"),
    )
    .context("failed to configure api-gateway outbox relay")?;
    tokio::spawn(relay.run());
    let key_client = Arc::new(CachedKeyClient::new(
        KeyClient {
            config: client_config("API_GATEWAY_KEY_SERVICES_GRPC_ENDPOINT")?,
        },
        std::env::var("API_GATEWAY_KEY_CACHE_TTL_SECONDS")
            .unwrap_or_else(|_| "60".to_string())
            .parse::<u64>()
            .context("API_GATEWAY_KEY_CACHE_TTL_SECONDS must be a valid u64")?,
    ));
    let app = Arc::new(ApiGatewayApplication {
        keys: key_client.clone(),
        plans: Arc::new(PlanClient {
            config: client_config("API_GATEWAY_SUBSCRIPTION_SERVICE_GRPC_ENDPOINT")?,
        }),
        catalog: Arc::new(CatalogClient {
            config: client_config("API_GATEWAY_CATALOG_GRPC_ENDPOINT")?,
        }),
        billing: Arc::new(BillingClient {
            config: client_config("API_GATEWAY_BILLING_GRPC_ENDPOINT")?,
        }),
        router: Arc::new(ModelRouterClient {
            config: client_config("API_GATEWAY_MODEL_ROUTER_GRPC_ENDPOINT")?,
        }),
        credentials: Arc::new(CredentialClient {
            config: client_config("API_GATEWAY_CREDENTIAL_MANAGER_GRPC_ENDPOINT")?,
        }),
        limiter: Arc::new(RedisRateLimiter { pool: redis }),
        events: Arc::new(GatewayEventPublisher { outbox }),
        bedrock: bedrock_client::Client::try_new()
            .map_err(|e| anyhow::anyhow!("failed to build bedrock client: {e}"))?,
        config: GatewayConfig {
            mantle_base_url: string_env("API_GATEWAY_MANTLE_BASE_URL")?,
            reserve_safety_multiplier: decimal_env("API_GATEWAY_RESERVE_SAFETY_MULTIPLIER")?,
        },
    });

    {
        let revoked_key_client = key_client.clone();
        let revoked_jetstream = jetstream.clone();
        tokio::spawn(async move {
            let stream = match revoked_jetstream
                .get_stream(events::jetstream::EVENTS_STREAM)
                .await
            {
                Ok(s) => s,
                Err(e) => {
                    tracing::error!(error = %e, "failed to open EVENTS stream");
                    return;
                }
            };
            let consumer_config = match events::jetstream::pull_consumer_config(
                "api-gateway-key-revocations",
                events::EVENT_API_KEY_REVOKED,
                100,
            ) {
                Ok(config) => config,
                Err(e) => {
                    tracing::error!(error = %e, "invalid key revocation consumer config");
                    return;
                }
            };
            let consumer = match stream.create_consumer(consumer_config).await {
                Ok(consumer) => consumer,
                Err(e) => {
                    tracing::error!(error = %e, "failed to create key revocation consumer");
                    return;
                }
            };
            loop {
                let mut batch = match consumer.batch().max_messages(100).messages().await {
                    Ok(batch) => batch,
                    Err(e) => {
                        tracing::error!(error = %e, "failed to fetch key revocation batch");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                };
                while let Some(result) = batch.next().await {
                    let message = match result {
                        Ok(message) => message,
                        Err(e) => {
                            tracing::warn!(error = %e, "key revocation message error");
                            continue;
                        }
                    };
                    let span = common::observability::nats_consumer_span(
                        message.subject.as_str(),
                        "api-gateway-key-revocations",
                        message.headers.as_ref(),
                    );
                    async {
                        match events::EventEnvelope::<events::ApiKeyRevoked>::from_json(
                            &message.payload,
                        ) {
                        Ok(envelope) => {
                            tracing::info!(
                                api_key_id = %envelope.payload.api_key_id,
                                "invalidating cached API key"
                            );
                            revoked_key_client.invalidate_user(envelope.payload.user_id);
                            if let Err(e) = events::jetstream::ack(&message).await {
                                tracing::warn!(error = %e, "failed to ACK key revocation");
                            }
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "invalid api_key.revoked event");
                            if let Err(dlq_error) = events::jetstream::dead_letter(
                                &revoked_jetstream,
                                &message,
                                "api-gateway-key-revocations",
                                e.to_string(),
                            )
                            .await
                            {
                                tracing::error!(error = %dlq_error, "failed to dead-letter invalid key revocation");
                            }
                        }
                        }
                    }
                    .instrument(span)
                    .await;
                }
            }
        });
    }

    // Subscription changes invalidate the gateway's plaintext-key cache in
    // addition to the key-service epoch. This closes the otherwise stale L1
    // authorization window when a plan is downgraded/canceled.
    {
        let subscription_client = key_client.clone();
        let subscription_jetstream = jetstream.clone();
        tokio::spawn(async move {
            let Ok(stream) = subscription_jetstream
                .get_stream(events::jetstream::EVENTS_STREAM)
                .await
            else {
                return;
            };
            let Ok(config) = events::jetstream::pull_consumer_config(
                "api-gateway-subscription-cache",
                events::EVENT_SUBSCRIPTION_CHANGED,
                100,
            ) else {
                return;
            };
            let Ok(consumer) = stream.create_consumer(config).await else {
                return;
            };
            loop {
                let Ok(mut batch) = consumer.batch().max_messages(100).messages().await else {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                };
                while let Some(Ok(message)) = batch.next().await {
                    if let Ok(envelope) =
                        events::EventEnvelope::<events::SubscriptionChanged>::from_json(
                            &message.payload,
                        )
                    {
                        subscription_client.invalidate_user(envelope.payload.user_id);
                        let _ = events::jetstream::ack(&message).await;
                    } else {
                        let _ = events::jetstream::dead_letter(
                            &subscription_jetstream,
                            &message,
                            "api-gateway-subscription-cache",
                            "invalid subscription.changed event",
                        )
                        .await;
                    }
                }
            }
        });
    }

    // Scope/metadata changes are propagated through the outbox so every
    // gateway instance drops its local authorization cache immediately.
    {
        let authorization_client = key_client.clone();
        let authorization_jetstream = jetstream.clone();
        tokio::spawn(async move {
            let Ok(stream) = authorization_jetstream
                .get_stream(events::jetstream::EVENTS_STREAM)
                .await
            else {
                return;
            };
            let Ok(config) = events::jetstream::pull_consumer_config(
                "api-gateway-key-authorization",
                events::EVENT_API_KEY_AUTHORIZATION_CHANGED,
                100,
            ) else {
                return;
            };
            let Ok(consumer) = stream.create_consumer(config).await else {
                return;
            };
            loop {
                let Ok(mut batch) = consumer.batch().max_messages(100).messages().await else {
                    tokio::time::sleep(Duration::from_secs(1)).await;
                    continue;
                };
                while let Some(Ok(message)) = batch.next().await {
                    if let Ok(envelope) =
                        events::EventEnvelope::<events::ApiKeyAuthorizationChanged>::from_json(
                            &message.payload,
                        )
                    {
                        authorization_client.invalidate_user(envelope.payload.user_id);
                        let _ = events::jetstream::ack(&message).await;
                    } else {
                        let _ = events::jetstream::dead_letter(
                            &authorization_jetstream,
                            &message,
                            "api-gateway-key-authorization",
                            "invalid api_key.authorization_changed event",
                        )
                        .await;
                    }
                }
            }
        });
    }

    let bind_addr =
        std::env::var("API_GATEWAY_BIND_ADDR").context("API_GATEWAY_BIND_ADDR is required")?;
    let address: SocketAddr = format!("{}:{}", bind_addr, required(config.http_port, "HTTP_PORT")?)
        .parse()
        .context("invalid api-gateway bind address")?;
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .context("failed to bind api-gateway HTTP server")?;
    tracing::info!(%address, "api-gateway HTTP listening");

    let metrics_handle = PrometheusBuilder::new()
        .install_recorder()
        .context("failed to install prometheus metrics recorder")?;

    axum::serve(
        listener,
        router(HttpState {
            app,
            metrics: Arc::new(metrics_handle),
        })
        .into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(common::shutdown::shutdown_signal())
    .await
    .context("api-gateway HTTP server failed")?;
    Ok(())
}
