//! Hot reload for AWS-backed configuration.
//!
//! The process keeps the last known-good snapshot and publishes a new snapshot
//! atomically through a watch channel. A failed refresh never replaces a valid
//! configuration and never falls back to Vault. PostgreSQL pools can update the
//! options used for future connections after a validated credential rotation;
//! existing connections are intentionally drained by their normal idle policy.

use crate::config::load_secret_configuration_async;
use crate::startup::ReloadableRedisPool;
use crate::AppConfig;
use async_nats::Client;
use chrono::{DateTime, Utc};
use sha2::{Digest, Sha256};
use sqlx::postgres::{PgConnectOptions, PgConnection};
use sqlx::{Connection, PgPool};
use std::collections::BTreeMap;
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{watch, RwLock};

const DEFAULT_POLL_SECONDS: u64 = 30;
const DEFAULT_MAX_STALE_SECONDS: u64 = 300;

#[derive(Clone)]
pub struct ConfigSnapshot {
    pub config: AppConfig,
    pub revision: String,
    pub loaded_at: DateTime<Utc>,
    environment: Arc<BTreeMap<String, String>>,
}

impl fmt::Debug for ConfigSnapshot {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfigSnapshot")
            .field("revision", &self.revision)
            .field("loaded_at", &self.loaded_at)
            .field("environment_keys", &self.environment.len())
            .finish_non_exhaustive()
    }
}

impl ConfigSnapshot {
    pub fn env_value(&self, name: &str) -> Option<&str> {
        self.environment.get(name).map(String::as_str)
    }
}

#[derive(Clone)]
pub struct ConfigReloadHandle {
    current: Arc<RwLock<Arc<ConfigSnapshot>>>,
    updates: watch::Sender<Arc<ConfigSnapshot>>,
}

impl fmt::Debug for ConfigReloadHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ConfigReloadHandle")
            .finish_non_exhaustive()
    }
}

impl ConfigReloadHandle {
    pub fn start(initial: AppConfig) -> Self {
        let snapshot = Arc::new(snapshot_from_environment(initial));
        let (updates, _) = watch::channel(Arc::clone(&snapshot));
        let handle = Self {
            current: Arc::new(RwLock::new(snapshot)),
            updates,
        };

        if reload_enabled() {
            let worker = handle.clone();
            tokio::spawn(async move {
                worker.run().await;
            });
            tracing::info!(
                interval_seconds = poll_seconds(),
                max_stale_seconds = max_stale_seconds(),
                "dynamic configuration reload enabled"
            );
        } else {
            tracing::info!("dynamic configuration reload disabled");
        }
        handle
    }

    pub async fn current(&self) -> Arc<ConfigSnapshot> {
        let snapshot = self.current.read().await;
        Arc::clone(&snapshot)
    }

    pub fn subscribe(&self) -> watch::Receiver<Arc<ConfigSnapshot>> {
        self.updates.subscribe()
    }

    /// Updates a PostgreSQL pool only after opening a probe connection with the
    /// new URL. Existing checked-out connections remain valid and new
    /// connections use the new options through SQLx's atomic setter.
    pub fn register_postgres_pool(
        &self,
        service: &'static str,
        database_url_key: &'static str,
        pool: PgPool,
    ) {
        let mut receiver = self.subscribe();
        tokio::spawn(async move {
            let mut active_url = std::env::var(database_url_key).ok();
            loop {
                if receiver.changed().await.is_err() {
                    return;
                }
                let snapshot = Arc::clone(&receiver.borrow_and_update());
                let Some(candidate) = snapshot
                    .env_value(database_url_key)
                    .or_else(|| {
                        (database_url_key == "DATABASE_URL")
                            .then_some(snapshot.config.database_url.as_str())
                    })
                    .map(str::to_owned)
                else {
                    tracing::error!(
                        service,
                        database_url_key,
                        "database URL disappeared during configuration reload"
                    );
                    continue;
                };
                if active_url.as_deref() == Some(candidate.as_str()) {
                    continue;
                }
                let options = match PgConnectOptions::from_str(&candidate) {
                    Ok(options) => options,
                    Err(error) => {
                        tracing::error!(service, database_url_key, error = %error, "reloaded PostgreSQL URL is invalid");
                        continue;
                    }
                };
                let probe = match PgConnection::connect_with(&options).await {
                    Ok(connection) => connection,
                    Err(error) => {
                        tracing::error!(service, database_url_key, error = %error, "reloaded PostgreSQL credentials failed validation");
                        continue;
                    }
                };
                if let Err(error) = probe.close().await {
                    tracing::warn!(service, database_url_key, error = %error, "PostgreSQL credential probe close failed");
                }
                pool.set_connect_options(options);
                active_url = Some(candidate);
                tracing::info!(
                    service,
                    database_url_key,
                    "PostgreSQL pool connection options rotated"
                );
            }
        });
    }

    pub fn register_redis_pool(
        &self,
        service: &'static str,
        redis_url_key: &'static str,
        pool: ReloadableRedisPool,
    ) {
        let mut receiver = self.subscribe();
        tokio::spawn(async move {
            let mut active_url = std::env::var(redis_url_key).ok();
            loop {
                if receiver.changed().await.is_err() {
                    return;
                }
                let snapshot = Arc::clone(&receiver.borrow_and_update());
                let Some(candidate) = snapshot.env_value(redis_url_key).map(str::to_owned) else {
                    tracing::error!(
                        service,
                        redis_url_key,
                        "Redis URL disappeared during configuration reload"
                    );
                    continue;
                };
                if active_url.as_deref() == Some(candidate.as_str()) {
                    continue;
                }
                match pool.replace_url(service, &candidate).await {
                    Ok(()) => active_url = Some(candidate),
                    Err(error) => {
                        tracing::error!(service, redis_url_key, error = %error, "reloaded Redis credentials failed validation; retaining current pool")
                    }
                }
            }
        });
    }

    pub fn register_nats_client(
        &self,
        service: &'static str,
        server_url_key: &'static str,
        client: Client,
    ) {
        crate::nats::supervise_nats_client(self, service, client, server_url_key);
    }

    async fn run(self) {
        let mut ticker = tokio::time::interval(Duration::from_secs(poll_seconds()));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            ticker.tick().await;
            match refresh_snapshot().await {
                Ok(next) => {
                    let next = Arc::new(next);
                    let previous = self.current().await;
                    if previous.revision == next.revision {
                        continue;
                    }
                    *self.current.write().await = Arc::clone(&next);
                    self.updates.send_replace(next);
                    tracing::info!("dynamic configuration snapshot replaced");
                    metrics::counter!("config_reload_success_total").increment(1);
                }
                Err(error) => {
                    let previous = self.current().await;
                    let stale_seconds =
                        (Utc::now() - previous.loaded_at).num_seconds().max(0) as u64;
                    metrics::counter!("config_reload_errors_total").increment(1);
                    if stale_seconds > max_stale_seconds() {
                        tracing::error!(stale_seconds, error = %error, "dynamic configuration is beyond maximum stale age; retaining last known-good snapshot");
                    } else {
                        tracing::warn!(stale_seconds, error = %error, "dynamic configuration refresh failed; retaining last known-good snapshot");
                    }
                }
            }
        }
    }
}

fn snapshot_from_environment(config: AppConfig) -> ConfigSnapshot {
    let environment = std::env::vars().collect::<BTreeMap<_, _>>();
    let revision = revision_for(&config, &environment);
    ConfigSnapshot {
        config,
        revision,
        loaded_at: Utc::now(),
        environment: Arc::new(environment),
    }
}

async fn refresh_snapshot() -> Result<ConfigSnapshot, String> {
    load_secret_configuration_async()
        .await
        .map_err(|error| error.to_string())?;
    let config = envy::from_env::<AppConfig>().map_err(|error| error.to_string())?;
    config.validate().map_err(|error| error.to_string())?;
    Ok(snapshot_from_environment(config))
}

fn revision_for(config: &AppConfig, environment: &BTreeMap<String, String>) -> String {
    let mut hasher = Sha256::new();
    if let Ok(serialized) = serde_json::to_vec(config) {
        hasher.update(serialized);
    }
    for (name, value) in environment {
        hasher.update(name.as_bytes());
        hasher.update([0]);
        hasher.update(value.as_bytes());
        hasher.update([0]);
    }
    hex::encode(hasher.finalize())
}

fn reload_enabled() -> bool {
    std::env::var("CONFIG_RELOAD_ENABLED")
        .ok()
        .and_then(|value| value.parse::<bool>().ok())
        .unwrap_or_else(|| {
            std::env::var("SECRETS_PROVIDER")
                .map(|value| value.eq_ignore_ascii_case("aws"))
                .unwrap_or(false)
        })
}

fn poll_seconds() -> u64 {
    std::env::var("CONFIG_RELOAD_INTERVAL_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value >= 5)
        .unwrap_or(DEFAULT_POLL_SECONDS)
}

fn max_stale_seconds() -> u64 {
    std::env::var("CONFIG_RELOAD_MAX_STALE_SECONDS")
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value >= poll_seconds())
        .unwrap_or(DEFAULT_MAX_STALE_SECONDS)
}

#[cfg(test)]
mod tests {
    use super::revision_for;
    use std::collections::BTreeMap;

    #[test]
    fn revision_changes_when_configuration_changes() {
        let mut first = BTreeMap::new();
        first.insert("CONFIG_RELOAD_ENABLED".to_string(), "true".to_string());
        let mut second = first.clone();
        second.insert(
            "CONFIG_RELOAD_INTERVAL_SECONDS".to_string(),
            "60".to_string(),
        );
        let entries = vec![
            (
                "DATABASE_URL".to_string(),
                "postgres://localhost/db".to_string(),
            ),
            ("REDIS_URL".to_string(), "redis://localhost".to_string()),
            ("NATS_URL".to_string(), "nats://localhost".to_string()),
        ];
        let parsed: Result<crate::AppConfig, _> = envy::from_iter(entries);
        assert!(
            parsed.is_ok(),
            "valid test configuration could not be parsed"
        );
        let config = match parsed {
            Ok(config) => config,
            Err(error) => {
                tracing::error!(%error, "valid test configuration could not be parsed");
                return;
            }
        };
        assert_ne!(
            revision_for(&config, &first),
            revision_for(&config, &second)
        );
    }
}
