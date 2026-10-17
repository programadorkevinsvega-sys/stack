use crate::application::ports::{AuditLogger, KmsCrypto, StsTokenProvider};
use crate::domain::entities::{
    AwsCredential, MantleConnection, NewStaticKeysCredential, NewStsAssumeRoleCredential,
};
use crate::domain::errors::CredentialError;
use crate::domain::repositories::{AwsAccountModelRepository, AwsCredentialRepository};
use common::AuditEntry;
use serde_json::json;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

pub struct UpsertMantleConnection {
    pub repo: Arc<dyn AwsAccountModelRepository>,
    pub kms: Arc<dyn KmsCrypto>,
    pub audit_logger: Arc<dyn AuditLogger>,
    pub kms_key_id: String,
}

impl UpsertMantleConnection {
    pub async fn execute(
        &self,
        account_id: Uuid,
        model_catalog_id: Uuid,
        api_key: String,
        base_url: String,
        expires_at: Option<chrono::DateTime<chrono::Utc>>,
        admin_actor_id: Uuid,
    ) -> Result<(), CredentialError> {
        if api_key.trim().is_empty() {
            return Err(CredentialError::Validation("api_key is required".into()));
        }
        let base_url = base_url.trim().trim_end_matches('/').to_string();
        let parsed = url::Url::parse(&base_url)
            .map_err(|e| CredentialError::Validation(format!("invalid base_url: {e}")))?;
        if parsed.scheme() != "https" || parsed.host_str().is_none() {
            return Err(CredentialError::Validation(
                "base_url must be an HTTPS URL".into(),
            ));
        }
        if self.kms_key_id.trim().is_empty() {
            return Err(CredentialError::Configuration(
                "MANTLE_API_KMS_KEY_ID is required".into(),
            ));
        }
        let encrypted = self
            .kms
            .encrypt(account_id, &self.kms_key_id, api_key.as_bytes())
            .await?;
        let connection = MantleConnection {
            account_id,
            model_catalog_id,
            api_key_encrypted: Some(encrypted),
            api_key_kms_key_id: Some(self.kms_key_id.clone()),
            base_url: Some(base_url),
            expires_at,
        };
        self.repo.save_mantle_connection(&connection).await?;
        self.audit_logger
            .record(AuditEntry {
                actor_type: "admin".into(),
                actor_id: Some(admin_actor_id),
                action: "mantle_connection.updated".into(),
                resource_type: "aws_account_model".into(),
                resource_id: Some(account_id),
                metadata: json!({"model_catalog_id": model_catalog_id, "base_url": connection.base_url, "expires_at": expires_at}),
                ip_address: None,
            })
            .await?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub enum RegisterAccountAuthInput {
    StsAssumeRole {
        role_arn: String,
        external_id: Option<String>,
    },
    StaticKeys {
        access_key_id: String,
        secret_access_key: String,
        kms_key_id: String,
    },
}

#[derive(Debug, Clone)]
pub struct RegisterAwsAccountInput {
    pub alias: String,
    pub aws_account_id: String,
    pub region: String,
    pub max_tpm: i32,
    pub max_rpm: i32,
    pub priority_weight: i16,
    pub auth: RegisterAccountAuthInput,
}

pub struct RegisterAwsAccount {
    pub repo: Arc<dyn AwsCredentialRepository>,
    pub kms: Arc<dyn KmsCrypto>,
    pub audit_logger: Arc<dyn AuditLogger>,
}

impl RegisterAwsAccount {
    pub async fn execute(
        &self,
        input: RegisterAwsAccountInput,
    ) -> Result<AwsCredential, CredentialError> {
        let aws_account_id = input.aws_account_id.clone();
        let credential = match input.auth {
            RegisterAccountAuthInput::StsAssumeRole {
                role_arn,
                external_id,
            } => AwsCredential::new_sts_assume_role(NewStsAssumeRoleCredential {
                account_alias: input.alias,
                aws_account_id: input.aws_account_id,
                region: input.region,
                role_arn,
                external_id,
                max_tpm: input.max_tpm,
                max_rpm: input.max_rpm,
                priority_weight: input.priority_weight,
            })?,
            RegisterAccountAuthInput::StaticKeys {
                access_key_id,
                secret_access_key,
                kms_key_id,
            } => {
                let account_id = Uuid::now_v7();
                let encrypted_access_key = self
                    .kms
                    .encrypt(account_id, &kms_key_id, access_key_id.as_bytes())
                    .await?;
                let encrypted_secret_key = self
                    .kms
                    .encrypt(account_id, &kms_key_id, secret_access_key.as_bytes())
                    .await?;

                AwsCredential::new_static_keys(NewStaticKeysCredential {
                    id: Some(account_id),
                    account_alias: input.alias,
                    aws_account_id: input.aws_account_id,
                    region: input.region,
                    access_key_encrypted: encrypted_access_key,
                    secret_key_encrypted: encrypted_secret_key,
                    kms_key_id,
                    max_tpm: input.max_tpm,
                    max_rpm: input.max_rpm,
                    priority_weight: input.priority_weight,
                })?
            }
        };

        let inserted = self.repo.save(&credential).await?;
        if !inserted {
            return Err(CredentialError::AccountAlreadyExists {
                account_id: aws_account_id,
            });
        }

        if let Err(e) = self
            .audit_logger
            .record(AuditEntry {
                actor_type: "system".into(),
                actor_id: None,
                action: "aws_account.registered".into(),
                resource_type: "aws_account".into(),
                resource_id: Some(credential.id),
                metadata: json!({"alias": credential.account_alias, "aws_account_id": credential.aws_account_id, "auth_method": credential.auth_method}),
                ip_address: None,
            })
            .await
        {
            warn!(error = %e, account_id = %credential.id, "Failed to record audit log for account registration");
        }

        info!(
            account_id = %credential.id,
            aws_account_id = %credential.aws_account_id,
            auth_method = %credential.auth_method,
            "AWS account registered"
        );
        Ok(credential)
    }
}

pub struct RemoveAwsAccount {
    pub repo: Arc<dyn AwsCredentialRepository>,
    pub sts_provider: Arc<dyn StsTokenProvider>,
    pub audit_logger: Arc<dyn AuditLogger>,
}

impl RemoveAwsAccount {
    pub async fn execute(&self, account_id: Uuid) -> Result<(), CredentialError> {
        let credential = self
            .repo
            .find_by_id(account_id)
            .await?
            .ok_or_else(|| CredentialError::AccountNotFound(account_id.to_string()))?;

        if !credential.is_active {
            return Err(CredentialError::AccountInactive(account_id.to_string()));
        }

        self.repo.soft_delete(account_id).await?;
        self.sts_provider.invalidate_cache(account_id).await?;

        if let Err(e) = self
            .audit_logger
            .record(AuditEntry {
                actor_type: "system".into(),
                actor_id: None,
                action: "aws_account.removed".into(),
                resource_type: "aws_account".into(),
                resource_id: Some(account_id),
                metadata: json!({"aws_account_id": credential.aws_account_id}),
                ip_address: None,
            })
            .await
        {
            warn!(error = %e, account_id = %account_id, "Failed to record audit log for account removal");
        }

        info!(account_id = %account_id, "AWS account removed");
        Ok(())
    }
}

pub struct UpdateAccountMetadata {
    pub repo: Arc<dyn AwsCredentialRepository>,
    pub audit_logger: Arc<dyn AuditLogger>,
}

impl UpdateAccountMetadata {
    pub async fn execute(
        &self,
        account_id: Uuid,
        max_tpm: i32,
        max_rpm: i32,
        priority_weight: i16,
    ) -> Result<(), CredentialError> {
        if max_tpm <= 0 || max_rpm <= 0 || priority_weight <= 0 {
            return Err(CredentialError::Validation(
                "max_tpm, max_rpm and priority_weight must be > 0".to_string(),
            ));
        }
        self.repo
            .update_metadata(account_id, max_tpm, max_rpm, priority_weight)
            .await?;

        if let Err(e) = self
            .audit_logger
            .record(AuditEntry {
                actor_type: "system".into(),
                actor_id: None,
                action: "aws_account.metadata_updated".into(),
                resource_type: "aws_account".into(),
                resource_id: Some(account_id),
                metadata: json!({"max_tpm": max_tpm, "max_rpm": max_rpm, "priority_weight": priority_weight}),
                ip_address: None,
            })
            .await
        {
            warn!(error = %e, account_id = %account_id, "Failed to record audit log for account metadata update");
        }

        Ok(())
    }
}

pub struct MarkUnhealthy {
    pub repo: Arc<dyn AwsCredentialRepository>,
    pub sts_provider: Arc<dyn StsTokenProvider>,
    pub audit_logger: Arc<dyn AuditLogger>,
}

impl MarkUnhealthy {
    pub async fn execute(
        &self,
        account_id: Uuid,
        error_code: String,
    ) -> Result<(), CredentialError> {
        let credential = self
            .repo
            .find_by_id(account_id)
            .await?
            .ok_or_else(|| CredentialError::AccountNotFound(account_id.to_string()))?;

        let new_count = credential.consecutive_errors.saturating_add(1);
        let is_healthy = new_count < 5;

        self.repo
            .update_health(account_id, is_healthy, new_count, Some(&error_code))
            .await?;

        if !is_healthy {
            self.sts_provider.invalidate_cache(account_id).await?;
        }

        if !is_healthy && credential.is_healthy {
            warn!(
                account_id = %account_id,
                aws_account_id = %credential.aws_account_id,
                consecutive_errors = new_count,
                "Account marked unhealthy"
            );

            if let Err(e) = self
                .audit_logger
                .record(AuditEntry {
                    actor_type: "system".into(),
                    actor_id: None,
                    action: "aws_account.marked_unhealthy".into(),
                    resource_type: "aws_account".into(),
                    resource_id: Some(account_id),
                    metadata: json!({"aws_account_id": credential.aws_account_id, "error_code": error_code, "consecutive_errors": new_count}),
                    ip_address: None,
                })
                .await
            {
                warn!(error = %e, account_id = %account_id, "Failed to record audit log for account unhealthy");
            }
        }

        Ok(())
    }
}

pub struct MarkRecovered {
    pub repo: Arc<dyn AwsCredentialRepository>,
    pub sts_provider: Arc<dyn StsTokenProvider>,
    pub audit_logger: Arc<dyn AuditLogger>,
}

impl MarkRecovered {
    pub async fn execute(&self, account_id: Uuid) -> Result<(), CredentialError> {
        let credential = self
            .repo
            .find_by_id(account_id)
            .await?
            .ok_or_else(|| CredentialError::AccountNotFound(account_id.to_string()))?;

        self.repo.update_health(account_id, true, 0, None).await?;
        self.sts_provider.invalidate_cache(account_id).await?;

        if let Err(e) = self
            .audit_logger
            .record(AuditEntry {
                actor_type: "system".into(),
                actor_id: None,
                action: "aws_account.marked_recovered".into(),
                resource_type: "aws_account".into(),
                resource_id: Some(account_id),
                metadata: json!({"aws_account_id": credential.aws_account_id}),
                ip_address: None,
            })
            .await
        {
            warn!(error = %e, account_id = %account_id, "Failed to record audit log for account recovery");
        }

        info!(
            account_id = %account_id,
            aws_account_id = %credential.aws_account_id,
            "Account recovered"
        );
        Ok(())
    }
}

pub struct AddModelToAccount {
    pub repo: Arc<dyn AwsAccountModelRepository>,
}

impl AddModelToAccount {
    pub async fn execute(
        &self,
        account_id: Uuid,
        model_catalog_id: Uuid,
    ) -> Result<(), CredentialError> {
        self.repo.save(account_id, model_catalog_id).await
    }
}

pub struct RemoveModelFromAccount {
    pub repo: Arc<dyn AwsAccountModelRepository>,
}

impl RemoveModelFromAccount {
    pub async fn execute(
        &self,
        account_id: Uuid,
        model_catalog_id: Uuid,
    ) -> Result<(), CredentialError> {
        self.repo.delete(account_id, model_catalog_id).await
    }
}
