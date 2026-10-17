use crate::application::ports::{KmsCrypto, StsSessionCache, StsTokenProvider};
use crate::domain::entities::{AuthMethod, AwsCredential, StsSessionToken};
use crate::domain::errors::CredentialError;
use async_trait::async_trait;
use aws_credential_types::Credentials;
use aws_sdk_kms::primitives::Blob;
use chrono::{DateTime, Duration, Utc};
use std::sync::Arc;
use uuid::Uuid;

pub struct AwsKmsCrypto {
    client: aws_sdk_kms::Client,
}

impl AwsKmsCrypto {
    pub fn new(client: aws_sdk_kms::Client) -> Self {
        Self { client }
    }
}

#[async_trait]
impl KmsCrypto for AwsKmsCrypto {
    async fn encrypt(
        &self,
        account_id: Uuid,
        kms_key_id: &str,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, CredentialError> {
        let output = self
            .client
            .encrypt()
            .key_id(kms_key_id)
            .plaintext(Blob::new(plaintext.to_vec()))
            .encryption_context("service", "credential-manager")
            .encryption_context("context_version", "1")
            .encryption_context("account_id", account_id.to_string())
            .send()
            .await
            .map_err(|e| CredentialError::KmsEncryptionFailed {
                account_id: account_id.to_string(),
                reason: e.to_string(),
            })?;

        let blob =
            output
                .ciphertext_blob()
                .ok_or_else(|| CredentialError::KmsEncryptionFailed {
                    account_id: account_id.to_string(),
                    reason: "missing ciphertext blob in KMS response".to_string(),
                })?;
        Ok(blob.as_ref().to_vec())
    }

    async fn decrypt(
        &self,
        account_id: Uuid,
        kms_key_id: &str,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, CredentialError> {
        let output = self
            .client
            .decrypt()
            .key_id(kms_key_id)
            .ciphertext_blob(Blob::new(ciphertext.to_vec()))
            .encryption_context("service", "credential-manager")
            .encryption_context("context_version", "1")
            .encryption_context("account_id", account_id.to_string())
            .send()
            .await
            .map_err(|e| CredentialError::KmsDecryptionFailed {
                account_id: account_id.to_string(),
                reason: e.to_string(),
            })?;

        let plaintext = output
            .plaintext()
            .ok_or_else(|| CredentialError::KmsDecryptionFailed {
                account_id: account_id.to_string(),
                reason: "missing plaintext in KMS response".to_string(),
            })?;

        Ok(plaintext.as_ref().to_vec())
    }
}

pub struct AwsStsTokenProvider {
    base_sdk_config: aws_config::SdkConfig,
    sts_client: aws_sdk_sts::Client,
    kms: Arc<dyn KmsCrypto>,
    cache: Arc<dyn StsSessionCache>,
    session_duration_seconds: i32,
    static_keys_ttl_seconds: i64,
    mantle_role_arn: Option<String>,
}

impl AwsStsTokenProvider {
    pub fn new(
        base_sdk_config: aws_config::SdkConfig,
        sts_client: aws_sdk_sts::Client,
        kms: Arc<dyn KmsCrypto>,
        cache: Arc<dyn StsSessionCache>,
        session_duration_seconds: i32,
        static_keys_ttl_seconds: i64,
        mantle_role_arn: Option<String>,
    ) -> Result<Self, CredentialError> {
        if !(900..=43200).contains(&session_duration_seconds) {
            return Err(CredentialError::Configuration(
                "STS session duration must be between 900 and 43200 seconds".to_string(),
            ));
        }
        if !(60..=86_400).contains(&static_keys_ttl_seconds) {
            return Err(CredentialError::Configuration(
                "STATIC_KEYS_CACHE_TTL_SECONDS must be between 60 and 86400 seconds".to_string(),
            ));
        }
        Ok(Self {
            base_sdk_config,
            sts_client,
            kms,
            cache,
            session_duration_seconds,
            static_keys_ttl_seconds,
            mantle_role_arn,
        })
    }

    async fn assume_role(
        &self,
        credential: &AwsCredential,
    ) -> Result<StsSessionToken, CredentialError> {
        let role_arn = credential
            .role_arn
            .as_deref()
            .ok_or_else(|| CredentialError::Validation("missing role_arn".to_string()))?;

        let session_name = format!("credmgr-{}", credential.id);
        let mut req = self
            .sts_client
            .assume_role()
            .role_arn(role_arn)
            .role_session_name(session_name)
            .duration_seconds(self.session_duration_seconds);

        if let Some(external_id) = credential.external_id.as_deref() {
            req = req.external_id(external_id);
        }

        let output = req
            .send()
            .await
            .map_err(|e| CredentialError::StsAssumeRoleFailed {
                account_id: credential.id.to_string(),
                reason: e.to_string(),
            })?;

        let creds = output
            .credentials()
            .ok_or_else(|| CredentialError::StsAssumeRoleFailed {
                account_id: credential.id.to_string(),
                reason: "STS response does not include credentials".to_string(),
            })?;

        let access_key_id = creds.access_key_id().to_string();
        let secret_access_key = creds.secret_access_key().to_string();
        let session_token = Some(creds.session_token().to_string());
        let expires_at = DateTime::<Utc>::from_timestamp(creds.expiration().secs(), 0);

        if expires_at.is_none() {
            return Err(CredentialError::StsAssumeRoleFailed {
                account_id: credential.id.to_string(),
                reason: "STS response missing valid expiration".to_string(),
            });
        }

        let token = StsSessionToken {
            account_id: credential.id,
            access_key_id,
            secret_access_key,
            session_token,
            expires_at,
            region: credential.region.clone(),
        };
        token.lock_memory();
        Ok(token)
    }

    async fn static_keys(
        &self,
        credential: &AwsCredential,
        verify_identity: bool,
    ) -> Result<StsSessionToken, CredentialError> {
        let kms_key_id = credential
            .kms_key_id
            .as_deref()
            .ok_or_else(|| CredentialError::Validation("missing kms_key_id".to_string()))?;

        let encrypted_access_key = credential.access_key_encrypted.as_deref().ok_or_else(|| {
            CredentialError::Validation("missing access_key_encrypted".to_string())
        })?;

        let encrypted_secret_key = credential.secret_key_encrypted.as_deref().ok_or_else(|| {
            CredentialError::Validation("missing secret_key_encrypted".to_string())
        })?;

        let access_key_id = String::from_utf8(
            self.kms
                .decrypt(credential.id, kms_key_id, encrypted_access_key)
                .await?,
        )
        .map_err(|e| CredentialError::KmsDecryptionFailed {
            account_id: credential.id.to_string(),
            reason: format!("access_key_id is not valid UTF-8: {e}"),
        })?;

        let secret_access_key = String::from_utf8(
            self.kms
                .decrypt(credential.id, kms_key_id, encrypted_secret_key)
                .await?,
        )
        .map_err(|e| CredentialError::KmsDecryptionFailed {
            account_id: credential.id.to_string(),
            reason: format!("secret_access_key is not valid UTF-8: {e}"),
        })?;

        if verify_identity {
            self.verify_static_keys_identity(credential, &access_key_id, &secret_access_key)
                .await?;
        }

        let token = StsSessionToken {
            account_id: credential.id,
            access_key_id,
            secret_access_key,
            session_token: None,
            expires_at: Some(Utc::now() + Duration::seconds(self.static_keys_ttl_seconds)),
            region: credential.region.clone(),
        };
        token.lock_memory();
        Ok(token)
    }

    async fn verify_static_keys_identity(
        &self,
        credential: &AwsCredential,
        access_key_id: &str,
        secret_access_key: &str,
    ) -> Result<(), CredentialError> {
        let static_credentials = Credentials::new(
            access_key_id,
            secret_access_key,
            None,
            None,
            "credential-manager-static-keys",
        );

        let sts_conf = aws_sdk_sts::config::Builder::from(&self.base_sdk_config)
            .credentials_provider(static_credentials)
            .region(aws_sdk_sts::config::Region::new(credential.region.clone()))
            .build();
        let static_sts_client = aws_sdk_sts::Client::from_conf(sts_conf);

        let identity = static_sts_client
            .get_caller_identity()
            .send()
            .await
            .map_err(|e| CredentialError::StsAssumeRoleFailed {
                account_id: credential.id.to_string(),
                reason: format!("static credentials validation failed: {e}"),
            })?;

        let reported_account = identity.account().unwrap_or_default();
        if reported_account != credential.aws_account_id {
            return Err(CredentialError::StsAssumeRoleFailed {
                account_id: credential.id.to_string(),
                reason: format!(
                    "static credentials account mismatch: expected {}, got {}",
                    credential.aws_account_id, reported_account
                ),
            });
        }

        Ok(())
    }

    async fn chain_assume_mantle_role(
        &self,
        base: &StsSessionToken,
    ) -> Result<StsSessionToken, CredentialError> {
        let Some(mantle_role_arn) = &self.mantle_role_arn else {
            return Ok(base.clone());
        };

        let chain_credentials = Credentials::new(
            &base.access_key_id,
            &base.secret_access_key,
            base.session_token.clone(),
            base.expires_at.map(|t| t.into()),
            "credential-manager-mantle-chain",
        );

        let chain_sts_conf = aws_sdk_sts::config::Builder::from(&self.base_sdk_config)
            .credentials_provider(chain_credentials)
            .region(aws_sdk_sts::config::Region::new(base.region.clone()))
            .build();
        let chain_sts = aws_sdk_sts::Client::from_conf(chain_sts_conf);

        let chain_duration = self.session_duration_seconds.min(3600);
        let output = chain_sts
            .assume_role()
            .role_arn(mantle_role_arn)
            .role_session_name(format!("mantle-chain-{}", base.account_id))
            .duration_seconds(chain_duration)
            .send()
            .await
            .map_err(|e| CredentialError::StsAssumeRoleFailed {
                account_id: base.account_id.to_string(),
                reason: format!("mantle role chaining failed: {e}"),
            })?;

        let creds = output
            .credentials()
            .ok_or_else(|| CredentialError::StsAssumeRoleFailed {
                account_id: base.account_id.to_string(),
                reason: "Mantle role chain response does not include credentials".to_string(),
            })?;

        let expires_at =
            DateTime::<Utc>::from_timestamp(creds.expiration().secs(), 0).ok_or_else(|| {
                CredentialError::StsAssumeRoleFailed {
                    account_id: base.account_id.to_string(),
                    reason: "Mantle role chain response missing valid expiration".to_string(),
                }
            })?;

        let token = StsSessionToken {
            account_id: base.account_id,
            access_key_id: creds.access_key_id().to_string(),
            secret_access_key: creds.secret_access_key().to_string(),
            session_token: Some(creds.session_token().to_string()),
            expires_at: Some(expires_at),
            region: base.region.clone(),
        };
        token.lock_memory();
        Ok(token)
    }

    async fn issue_token(
        &self,
        credential: &AwsCredential,
        use_cache: bool,
    ) -> Result<StsSessionToken, CredentialError> {
        if use_cache {
            if let Some(token) = self.cache.get_valid(credential.id).await {
                return Ok(token);
            }
        }

        let base = match credential.auth_method()? {
            AuthMethod::StsAssumeRole => self.assume_role(credential).await?,
            AuthMethod::StaticKeys => self.static_keys(credential, !use_cache).await?,
        };

        let token = self.chain_assume_mantle_role(&base).await?;
        self.cache.set(token.clone()).await;
        Ok(token)
    }
}

#[async_trait]
impl StsTokenProvider for AwsStsTokenProvider {
    async fn get_session_token(
        &self,
        credential: &AwsCredential,
    ) -> Result<StsSessionToken, CredentialError> {
        self.issue_token(credential, true).await
    }

    async fn get_session_token_fresh(
        &self,
        credential: &AwsCredential,
    ) -> Result<StsSessionToken, CredentialError> {
        self.issue_token(credential, false).await
    }

    async fn invalidate_cache(&self, account_id: Uuid) -> Result<(), CredentialError> {
        self.cache.invalidate(account_id).await;
        Ok(())
    }
}
