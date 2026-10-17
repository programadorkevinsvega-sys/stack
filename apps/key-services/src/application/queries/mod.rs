use crate::application::ports::{ApiKeyCache, KeyGenerator};
use crate::domain::entities::ApiKey;
use crate::domain::errors::KeyServiceError;
use crate::domain::repositories::ApiKeyRepository;
use chrono::Utc;
use std::net::IpAddr;
use std::sync::Arc;
use uuid::Uuid;

pub struct ValidateKey {
    pub repo: Arc<dyn ApiKeyRepository>,
    pub cache: Arc<dyn ApiKeyCache>,
    pub generator: Arc<dyn KeyGenerator>,
}
impl ValidateKey {
    pub async fn execute(
        &self,
        plaintext: &str,
        source_ip: Option<IpAddr>,
    ) -> Result<ApiKey, KeyServiceError> {
        let hash = self.generator.hash(plaintext)?;
        let key = match self.cache.get(&hash).await? {
            Some(key) => key,
            None => {
                let key = self
                    .repo
                    .find_by_hash(&hash)
                    .await?
                    .ok_or(KeyServiceError::ApiKeyNotFound)?;
                self.cache.set(&key).await?;
                key
            }
        };
        key.is_valid_for(source_ip, Utc::now())?;
        {
            let repo = Arc::clone(&self.repo);
            let key_id = key.id;
            tokio::spawn(async move {
                let _ = repo.update_last_used(key_id, source_ip).await;
            });
        }
        Ok(key)
    }
}
pub struct ListApiKeys {
    pub repo: Arc<dyn ApiKeyRepository>,
}
impl ListApiKeys {
    pub async fn execute(&self, user_id: Uuid) -> Result<Vec<ApiKey>, KeyServiceError> {
        self.repo.find_by_user(user_id).await
    }
}
pub struct GetApiKey {
    pub repo: Arc<dyn ApiKeyRepository>,
}
impl GetApiKey {
    pub async fn execute(&self, id: Uuid, user_id: Uuid) -> Result<ApiKey, KeyServiceError> {
        let key = self
            .repo
            .find_by_id(id)
            .await?
            .ok_or(KeyServiceError::ApiKeyNotFound)?;
        if key.user_id != user_id {
            return Err(KeyServiceError::ApiKeyNotFound);
        }
        Ok(key)
    }
}
