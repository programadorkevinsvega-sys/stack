use thiserror::Error;

#[derive(Debug, Error)]
pub enum ModelRouterError {
    #[error("no healthy account for model")]
    NoHealthyAccountForModel,
    #[error("all accounts circuit open")]
    AllAccountsCircuitOpen,
    #[error("account not found")]
    AccountNotFound,
    #[error("catalog model not found")]
    ModelNotFound,
    #[error("validation error: {0}")]
    Validation(String),
    #[error("dependency unavailable: {0}")]
    DependencyUnavailable(String),
}
