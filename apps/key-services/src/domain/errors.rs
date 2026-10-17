use thiserror::Error;

#[derive(Debug, Error)]
pub enum KeyServiceError {
    #[error("api key not found")]
    ApiKeyNotFound,
    #[error("api key revoked")]
    ApiKeyRevoked,
    #[error("api key expired")]
    ApiKeyExpired,
    #[error("api key already revoked")]
    KeyAlreadyRevoked,
    #[error("maximum active keys reached")]
    MaxKeysPerUserReached,
    #[error("an active subscription is required")]
    SubscriptionRequired,
    #[error("invalid scope: {0}")]
    InvalidScope(String),
    #[error("ip address is not allowed")]
    IpNotAllowed,
    #[error("dependency unavailable: {0}")]
    DependencyUnavailable(String),
    #[error("configuration error: {0}")]
    Configuration(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("rate limited: {0}")]
    RateLimit(String),
}
