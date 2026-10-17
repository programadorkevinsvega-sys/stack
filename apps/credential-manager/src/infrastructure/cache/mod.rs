pub mod redis;

use crate::application::ports::StsSessionCache;
use crate::domain::entities::StsSessionToken;
use async_trait::async_trait;
use std::collections::HashMap;
use tokio::sync::RwLock;
use uuid::Uuid;
use zeroize::Zeroize;

pub struct InMemoryStsSessionCache {
    inner: RwLock<HashMap<Uuid, StsSessionToken>>,
    refresh_buffer_seconds: i64,
}

impl InMemoryStsSessionCache {
    pub fn new(refresh_buffer_seconds: i64) -> Self {
        Self {
            inner: RwLock::new(HashMap::new()),
            refresh_buffer_seconds,
        }
    }
}

#[async_trait]
impl StsSessionCache for InMemoryStsSessionCache {
    async fn get_valid(&self, account_id: Uuid) -> Option<StsSessionToken> {
        let cache = self.inner.read().await;
        cache.get(&account_id).and_then(|token| {
            if token.is_expiring_within(self.refresh_buffer_seconds) {
                None
            } else {
                let cloned = token.clone();
                cloned.lock_memory();
                Some(cloned)
            }
        })
    }

    async fn set(&self, token: StsSessionToken) {
        let mut cache = self.inner.write().await;
        if let Some(mut old) = cache.insert(token.account_id, token) {
            old.zeroize();
        }
    }

    async fn invalidate(&self, account_id: Uuid) {
        let mut cache = self.inner.write().await;
        if let Some(mut token) = cache.remove(&account_id) {
            token.zeroize();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};

    fn token(account_id: Uuid, expires_in_sec: i64) -> StsSessionToken {
        StsSessionToken {
            account_id,
            access_key_id: "AKIA...".to_string(),
            secret_access_key: "SECRET".to_string(),
            session_token: Some("SESSION".to_string()),
            expires_at: Some(Utc::now() + Duration::seconds(expires_in_sec)),
            region: "us-east-1".to_string(),
        }
    }

    fn make_cache(refresh_buffer: i64) -> InMemoryStsSessionCache {
        InMemoryStsSessionCache::new(refresh_buffer)
    }

    #[tokio::test]
    async fn returns_token_when_not_expiring() {
        let cache = make_cache(60);
        let id = Uuid::new_v4();
        cache.set(token(id, 600)).await;
        assert!(cache.get_valid(id).await.is_some());
    }

    #[tokio::test]
    async fn expires_token_inside_refresh_buffer() {
        let cache = make_cache(60);
        let id = Uuid::new_v4();
        cache.set(token(id, 30)).await;
        assert!(cache.get_valid(id).await.is_none());
    }

    #[tokio::test]
    async fn invalidate_removes_token() {
        let cache = make_cache(60);
        let id = Uuid::new_v4();
        cache.set(token(id, 600)).await;
        cache.invalidate(id).await;
        assert!(cache.get_valid(id).await.is_none());
    }
}
