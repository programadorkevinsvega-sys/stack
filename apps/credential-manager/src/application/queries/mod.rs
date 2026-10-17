use crate::application::ports::{KmsCrypto, StsTokenProvider};
use crate::domain::entities::{AwsCredential, MantleConnection, StsSessionToken};
use crate::domain::errors::CredentialError;
use crate::domain::repositories::{AwsAccountModelRepository, AwsCredentialRepository};
use std::sync::Arc;
use uuid::Uuid;

pub struct GetMantleConnection {
    pub repo: Arc<dyn AwsAccountModelRepository>,
    pub kms: Arc<dyn KmsCrypto>,
}

pub struct MantleConnectionSecret {
    pub connection: MantleConnection,
    pub api_key: Option<String>,
}

impl GetMantleConnection {
    pub async fn execute(
        &self,
        account_id: Uuid,
        model_catalog_id: Uuid,
    ) -> Result<Option<MantleConnectionSecret>, CredentialError> {
        let connection = self
            .repo
            .get_mantle_connection(account_id, model_catalog_id)
            .await?;
        let Some(connection) = connection else {
            return Ok(None);
        };
        let api_key = match (
            connection.api_key_encrypted.as_deref(),
            connection.api_key_kms_key_id.as_deref(),
        ) {
            (Some(ciphertext), Some(kms_key_id)) => Some(
                String::from_utf8(self.kms.decrypt(account_id, kms_key_id, ciphertext).await?)
                    .map_err(|e| {
                        CredentialError::Validation(format!("invalid stored Mantle key: {e}"))
                    })?,
            ),
            (None, None) => None,
            _ => {
                return Err(CredentialError::Validation(
                    "incomplete stored Mantle connection".into(),
                ))
            }
        };
        Ok(Some(MantleConnectionSecret {
            connection,
            api_key,
        }))
    }
}

pub struct GetCredentials {
    pub repo: Arc<dyn AwsCredentialRepository>,
    pub sts_provider: Arc<dyn StsTokenProvider>,
}

impl GetCredentials {
    pub async fn execute(&self, account_id: Uuid) -> Result<StsSessionToken, CredentialError> {
        let credential = self
            .repo
            .find_by_id(account_id)
            .await?
            .ok_or_else(|| CredentialError::AccountNotFound(account_id.to_string()))?;

        credential.validate()?;

        if !credential.is_active {
            return Err(CredentialError::AccountInactive(account_id.to_string()));
        }
        if !credential.is_healthy {
            return Err(CredentialError::AccountUnhealthy(account_id.to_string()));
        }

        self.sts_provider.get_session_token(&credential).await
    }
}

pub struct ListHealthyAccountsForModel {
    pub repo: Arc<dyn AwsCredentialRepository>,
}

impl ListHealthyAccountsForModel {
    pub async fn execute(
        &self,
        model_catalog_id: Uuid,
    ) -> Result<Vec<AwsCredential>, CredentialError> {
        let mut accounts = self
            .repo
            .find_healthy_accounts_for_model(model_catalog_id)
            .await?;
        accounts.sort_by_key(|a| -a.priority_weight);
        Ok(accounts)
    }
}

pub struct GetAccount {
    pub repo: Arc<dyn AwsCredentialRepository>,
}

impl GetAccount {
    pub async fn execute(&self, id: Uuid) -> Result<Option<AwsCredential>, CredentialError> {
        self.repo.find_by_id(id).await
    }
}

pub struct ListAccounts {
    pub repo: Arc<dyn AwsCredentialRepository>,
}

pub struct ListModelsForAccount {
    pub repo: Arc<dyn AwsAccountModelRepository>,
}

impl ListModelsForAccount {
    pub async fn execute(&self, account_id: Uuid) -> Result<Vec<Uuid>, CredentialError> {
        self.repo.find_models_by_account(account_id).await
    }
}

impl ListAccounts {
    pub async fn execute(&self) -> Result<Vec<AwsCredential>, CredentialError> {
        self.repo.find_all_active().await
    }
}
