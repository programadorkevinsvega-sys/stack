use crate::domain::errors::GatewayError;
use crate::infrastructure::clients::{KeyClient, ValidatedKey};
use moka::sync::Cache;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Duration;
use uuid::Uuid;

#[derive(Clone)]
pub struct CachedKeyClient {
    inner: KeyClient,
    cache: Arc<Cache<String, ValidatedKey>>,
    by_user: Arc<Mutex<HashMap<Uuid, HashSet<String>>>>,
}

impl CachedKeyClient {
    pub fn new(inner: KeyClient, ttl_seconds: u64) -> Self {
        Self {
            inner,
            cache: Arc::new(
                Cache::builder()
                    .time_to_live(Duration::from_secs(ttl_seconds))
                    .max_capacity(100_000)
                    .build(),
            ),
            by_user: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub async fn validate(
        &self,
        api_key: &str,
        source_ip: Option<String>,
    ) -> Result<ValidatedKey, GatewayError> {
        if let Some(cached) = self.cache.get(api_key) {
            if cached.status == "active" {
                return Ok(cached);
            }
        }
        let validated = self.inner.validate(api_key, source_ip).await?;
        if validated.status == "active" {
            self.cache.insert(api_key.to_string(), validated.clone());
            if let Ok(mut index) = self.by_user.lock() {
                index
                    .entry(validated.user_id)
                    .or_default()
                    .insert(api_key.to_string());
            }
        }
        Ok(validated)
    }

    pub fn invalidate(&self, api_key: &str) {
        self.cache.invalidate(api_key);
    }

    pub fn invalidate_user(&self, user_id: Uuid) {
        if let Ok(mut index) = self.by_user.lock() {
            if let Some(keys) = index.remove(&user_id) {
                for key in keys {
                    self.cache.invalidate(&key);
                }
            }
        }
    }
}
