use super::entities::{AwsCredential, MantleConnection};
use super::errors::CredentialError;
use async_trait::async_trait;
use uuid::Uuid;

#[async_trait]
pub trait AwsCredentialRepository: Send + Sync {
    async fn save(&self, cred: &AwsCredential) -> Result<bool, CredentialError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<AwsCredential>, CredentialError>;
    async fn find_by_aws_account_id(
        &self,
        aws_account_id: &str,
    ) -> Result<Option<AwsCredential>, CredentialError>;
    async fn find_healthy_accounts_for_model(
        &self,
        model_catalog_id: Uuid,
    ) -> Result<Vec<AwsCredential>, CredentialError>;
    async fn find_all_active(&self) -> Result<Vec<AwsCredential>, CredentialError>;
    async fn update_health(
        &self,
        id: Uuid,
        is_healthy: bool,
        consecutive_errors: i16,
        error_code: Option<&str>,
    ) -> Result<(), CredentialError>;
    async fn update_metadata(
        &self,
        id: Uuid,
        max_tpm: i32,
        max_rpm: i32,
        priority_weight: i16,
    ) -> Result<(), CredentialError>;
    async fn soft_delete(&self, id: Uuid) -> Result<(), CredentialError>;
}

#[async_trait]
pub trait AwsAccountModelRepository: Send + Sync {
    async fn save(&self, account_id: Uuid, model_catalog_id: Uuid) -> Result<(), CredentialError>;
    async fn delete(&self, account_id: Uuid, model_catalog_id: Uuid)
        -> Result<(), CredentialError>;
    async fn find_models_by_account(&self, account_id: Uuid) -> Result<Vec<Uuid>, CredentialError>;
    async fn get_mantle_connection(
        &self,
        account_id: Uuid,
        model_catalog_id: Uuid,
    ) -> Result<Option<MantleConnection>, CredentialError>;
    async fn save_mantle_connection(
        &self,
        connection: &MantleConnection,
    ) -> Result<(), CredentialError>;
}
