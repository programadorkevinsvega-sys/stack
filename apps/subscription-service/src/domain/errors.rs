use thiserror::Error;
#[derive(Debug, Error)]
pub enum SubscriptionError {
    #[error("plan not found")]
    PlanNotFound,
    #[error("subscription not found")]
    SubscriptionNotFound,
    #[error("subscription not active")]
    SubscriptionNotActive,
    #[error("{0}")]
    Validation(String),
    #[error("dependency unavailable: {0}")]
    DependencyUnavailable(String),
}
