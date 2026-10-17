use crate::domain::errors::CredentialError;
use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

pub const AUTH_METHOD_STS_ASSUME_ROLE: &str = "sts_assume_role";
pub const AUTH_METHOD_STATIC_KEYS: &str = "static_keys";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthMethod {
    StsAssumeRole,
    StaticKeys,
}

impl AuthMethod {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::StsAssumeRole => AUTH_METHOD_STS_ASSUME_ROLE,
            Self::StaticKeys => AUTH_METHOD_STATIC_KEYS,
        }
    }

    pub fn from_db(value: &str) -> Result<Self, CredentialError> {
        match value {
            AUTH_METHOD_STS_ASSUME_ROLE => Ok(Self::StsAssumeRole),
            AUTH_METHOD_STATIC_KEYS => Ok(Self::StaticKeys),
            _ => Err(CredentialError::InvalidAuthMethod(value.to_string())),
        }
    }
}

#[derive(Debug, Clone)]
pub struct NewStsAssumeRoleCredential {
    pub account_alias: String,
    pub aws_account_id: String,
    pub region: String,
    pub role_arn: String,
    pub external_id: Option<String>,
    pub max_tpm: i32,
    pub max_rpm: i32,
    pub priority_weight: i16,
}

#[derive(Debug, Clone)]
pub struct NewStaticKeysCredential {
    pub id: Option<Uuid>,
    pub account_alias: String,
    pub aws_account_id: String,
    pub region: String,
    pub access_key_encrypted: Vec<u8>,
    pub secret_key_encrypted: Vec<u8>,
    pub kms_key_id: String,
    pub max_tpm: i32,
    pub max_rpm: i32,
    pub priority_weight: i16,
}

#[derive(Debug, Clone, FromRow)]
pub struct AwsCredential {
    pub id: Uuid,
    pub account_alias: String,
    pub aws_account_id: String,
    pub region: String,
    pub auth_method: String,
    pub role_arn: Option<String>,
    pub external_id: Option<String>,
    pub access_key_encrypted: Option<Vec<u8>>,
    pub secret_key_encrypted: Option<Vec<u8>>,
    pub kms_key_id: Option<String>,
    pub max_tpm: i32,
    pub max_rpm: i32,
    pub priority_weight: i16,
    pub is_healthy: bool,
    pub consecutive_errors: i16,
    pub last_error_at: Option<DateTime<Utc>>,
    pub last_error_code: Option<String>,
    pub last_health_check_at: Option<DateTime<Utc>>,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
}

impl AwsCredential {
    pub fn validate(&self) -> Result<(), CredentialError> {
        if self.account_alias.trim().is_empty() {
            return Err(CredentialError::Validation(
                "account_alias cannot be empty".to_string(),
            ));
        }
        if !self.aws_account_id.chars().all(|c| c.is_ascii_digit())
            || self.aws_account_id.len() != 12
        {
            return Err(CredentialError::Validation(
                "aws_account_id must be a 12-digit numeric string".to_string(),
            ));
        }
        if self.region.trim().is_empty() {
            return Err(CredentialError::Validation(
                "region cannot be empty".to_string(),
            ));
        }

        if self.max_tpm <= 0 {
            return Err(CredentialError::Validation(
                "max_tpm must be greater than 0".to_string(),
            ));
        }
        if self.max_rpm <= 0 {
            return Err(CredentialError::Validation(
                "max_rpm must be greater than 0".to_string(),
            ));
        }
        if self.priority_weight <= 0 {
            return Err(CredentialError::Validation(
                "priority_weight must be greater than 0".to_string(),
            ));
        }

        match self.auth_method()? {
            AuthMethod::StsAssumeRole => {
                if self.role_arn.as_deref().unwrap_or_default().is_empty() {
                    return Err(CredentialError::Validation(
                        "role_arn is required when auth_method=sts_assume_role".to_string(),
                    ));
                }
                if !self
                    .role_arn
                    .as_deref()
                    .map(|r| {
                        r.starts_with("arn:aws:iam::")
                            && r.contains(":role/")
                            && r.split('/').count() >= 2
                    })
                    .unwrap_or(false)
                {
                    return Err(CredentialError::Validation(
                        "role_arn must be a valid IAM role ARN".to_string(),
                    ));
                }
                if self.access_key_encrypted.is_some() || self.secret_key_encrypted.is_some() {
                    return Err(CredentialError::Validation(
                        "static encrypted keys are not allowed when auth_method=sts_assume_role"
                            .to_string(),
                    ));
                }
            }
            AuthMethod::StaticKeys => {
                if self
                    .access_key_encrypted
                    .as_ref()
                    .map(|v| v.is_empty())
                    .unwrap_or(true)
                {
                    return Err(CredentialError::Validation(
                        "access_key_encrypted is required when auth_method=static_keys".to_string(),
                    ));
                }
                if self
                    .secret_key_encrypted
                    .as_ref()
                    .map(|v| v.is_empty())
                    .unwrap_or(true)
                {
                    return Err(CredentialError::Validation(
                        "secret_key_encrypted is required when auth_method=static_keys".to_string(),
                    ));
                }
                if self.kms_key_id.as_deref().unwrap_or_default().is_empty() {
                    return Err(CredentialError::Validation(
                        "kms_key_id is required when auth_method=static_keys".to_string(),
                    ));
                }
            }
        }

        Ok(())
    }

    pub fn auth_method(&self) -> Result<AuthMethod, CredentialError> {
        AuthMethod::from_db(&self.auth_method)
    }

    pub fn new_sts_assume_role(input: NewStsAssumeRoleCredential) -> Result<Self, CredentialError> {
        let cred = Self {
            id: Uuid::now_v7(),
            account_alias: input.account_alias,
            aws_account_id: input.aws_account_id,
            region: input.region,
            auth_method: AUTH_METHOD_STS_ASSUME_ROLE.to_string(),
            role_arn: Some(input.role_arn),
            external_id: input.external_id,
            access_key_encrypted: None,
            secret_key_encrypted: None,
            kms_key_id: None,
            max_tpm: input.max_tpm,
            max_rpm: input.max_rpm,
            priority_weight: input.priority_weight,
            is_healthy: true,
            consecutive_errors: 0,
            last_error_at: None,
            last_error_code: None,
            last_health_check_at: None,
            is_active: true,
            created_at: Utc::now(),
        };
        cred.validate()?;
        Ok(cred)
    }

    pub fn new_static_keys(input: NewStaticKeysCredential) -> Result<Self, CredentialError> {
        let id = input.id.unwrap_or_else(Uuid::now_v7);
        let cred = Self {
            id,
            account_alias: input.account_alias,
            aws_account_id: input.aws_account_id,
            region: input.region,
            auth_method: AUTH_METHOD_STATIC_KEYS.to_string(),
            role_arn: None,
            external_id: None,
            access_key_encrypted: Some(input.access_key_encrypted),
            secret_key_encrypted: Some(input.secret_key_encrypted),
            kms_key_id: Some(input.kms_key_id),
            max_tpm: input.max_tpm,
            max_rpm: input.max_rpm,
            priority_weight: input.priority_weight,
            is_healthy: true,
            consecutive_errors: 0,
            last_error_at: None,
            last_error_code: None,
            last_health_check_at: None,
            is_active: true,
            created_at: Utc::now(),
        };
        cred.validate()?;
        Ok(cred)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_static_keys_without_kms_key_id() {
        let credential = AwsCredential {
            id: Uuid::now_v7(),
            account_alias: "pool-a".to_string(),
            aws_account_id: "123456789012".to_string(),
            region: "us-east-1".to_string(),
            auth_method: AUTH_METHOD_STATIC_KEYS.to_string(),
            role_arn: None,
            external_id: None,
            access_key_encrypted: Some(vec![1]),
            secret_key_encrypted: Some(vec![2]),
            kms_key_id: None,
            max_tpm: 100,
            max_rpm: 10,
            priority_weight: 100,
            is_healthy: true,
            consecutive_errors: 0,
            last_error_at: None,
            last_error_code: None,
            last_health_check_at: None,
            is_active: true,
            created_at: Utc::now(),
        };

        assert!(credential.validate().is_err());
    }

    #[test]
    fn accepts_sts_credentials_with_role_arn() {
        let credential = AwsCredential::new_sts_assume_role(NewStsAssumeRoleCredential {
            account_alias: "pool-a".to_string(),
            aws_account_id: "123456789012".to_string(),
            region: "us-east-1".to_string(),
            role_arn: "arn:aws:iam::123456789012:role/ai-gateway".to_string(),
            external_id: Some("external-id".to_string()),
            max_tpm: 10_000,
            max_rpm: 200,
            priority_weight: 100,
        });

        assert!(credential.is_ok());
    }
}
