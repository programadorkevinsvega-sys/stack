use thiserror::Error;

#[derive(Debug, Error)]
pub enum CredentialError {
    #[error("account not found: {0}")]
    AccountNotFound(String),

    #[error("account inactive: {0}")]
    AccountInactive(String),

    #[error("account unhealthy: {0}")]
    AccountUnhealthy(String),

    #[error("account already exists for aws_account_id={account_id}")]
    AccountAlreadyExists { account_id: String },

    #[error("invalid auth_method: {0}")]
    InvalidAuthMethod(String),

    #[error("kms encryption failed for account {account_id}: {reason}")]
    KmsEncryptionFailed { account_id: String, reason: String },

    #[error("kms decryption failed for account {account_id}: {reason}")]
    KmsDecryptionFailed { account_id: String, reason: String },

    #[error("sts assume role failed for account {account_id}: {reason}")]
    StsAssumeRoleFailed { account_id: String, reason: String },

    #[error("credential expired for account {account_id}")]
    CredentialExpired { account_id: String },

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("configuration error: {0}")]
    Configuration(String),

    #[error("dependency unavailable: {0}")]
    DependencyUnavailable(String),

    #[error("invalid data: {0}")]
    Validation(String),
}
