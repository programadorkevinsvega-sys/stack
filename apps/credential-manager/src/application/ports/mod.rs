use crate::domain::entities::{AwsCredential, StsSessionToken};
use crate::domain::errors::CredentialError;
use async_trait::async_trait;
use common::AuditEntry;
use uuid::Uuid;

#[async_trait]
pub trait StsSessionCache: Send + Sync {
    async fn get_valid(&self, account_id: Uuid) -> Option<StsSessionToken>;
    async fn set(&self, token: StsSessionToken);
    async fn invalidate(&self, account_id: Uuid);
}

#[async_trait]
pub trait AuditLogger: Send + Sync {
    async fn record(&self, entry: AuditEntry) -> Result<(), CredentialError>;
}

#[async_trait]
pub trait KmsCrypto: Send + Sync {
    async fn encrypt(
        &self,
        account_id: Uuid,
        kms_key_id: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CredentialError>;

    async fn decrypt(
        &self,
        account_id: Uuid,
        kms_key_id: &str,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CredentialError>;
}

#[async_trait]
pub trait StsTokenProvider: Send + Sync {
    async fn get_session_token(
        &self,
        credential: &AwsCredential,
    ) -> Result<StsSessionToken, CredentialError>;
    async fn get_session_token_fresh(
        &self,
        credential: &AwsCredential,
    ) -> Result<StsSessionToken, CredentialError>;

    async fn invalidate_cache(&self, account_id: Uuid) -> Result<(), CredentialError>;
}
