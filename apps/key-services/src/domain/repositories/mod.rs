use crate::domain::entities::ApiKey;
use crate::domain::errors::KeyServiceError;
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait ApiKeyRepository: Send + Sync {
    async fn save(&self, key: &ApiKey) -> Result<(), KeyServiceError>;
    async fn save_if_capacity(
        &self,
        key: &ApiKey,
        max_active_keys: i64,
    ) -> Result<bool, KeyServiceError> {
        if max_active_keys > 0 && self.count_active_by_user(key.user_id).await? >= max_active_keys {
            return Ok(false);
        }
        self.save(key).await?;
        Ok(true)
    }
    async fn find_by_id(&self, id: Uuid) -> Result<Option<ApiKey>, KeyServiceError>;
    async fn find_by_hash(&self, hash: &str) -> Result<Option<ApiKey>, KeyServiceError>;
    async fn find_by_user(&self, user_id: Uuid) -> Result<Vec<ApiKey>, KeyServiceError>;
    async fn count_active_by_user(&self, user_id: Uuid) -> Result<i64, KeyServiceError>;
    async fn revoke(&self, id: Uuid, revoked_by: Uuid, reason: &str)
        -> Result<(), KeyServiceError>;
    async fn update_last_used(
        &self,
        id: Uuid,
        source_ip: Option<std::net::IpAddr>,
    ) -> Result<(), KeyServiceError>;
    async fn update(&self, key: &ApiKey) -> Result<(), KeyServiceError>;
}
