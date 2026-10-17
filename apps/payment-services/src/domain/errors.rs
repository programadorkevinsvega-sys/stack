use thiserror::Error;

#[derive(Debug, Error)]
pub enum PaymentError {
    #[error("payment method not found")]
    PaymentMethodNotFound,
    #[error("payment provider event not found")]
    PaymentProviderEventNotFound,
    #[error("duplicate provider event")]
    DuplicateProviderEvent,
    #[error("invalid webhook signature")]
    InvalidWebhookSignature,
    #[error("payment failed: {0}")]
    PaymentFailed(String),
    #[error("provider error: {0}")]
    ProviderError(String),
    #[error("{0}")]
    Validation(String),
    #[error("dependency unavailable: {0}")]
    DependencyUnavailable(String),
}
