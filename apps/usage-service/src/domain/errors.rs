use thiserror::Error;

#[derive(Debug, Error)]
pub enum UsageError {
    #[error("duplicate request_id")]
    DuplicateRequestId,
    #[error("invalid time range")]
    InvalidTimeRange,
    #[error("invalid usage event: {0}")]
    Validation(String),
    #[error("dependency unavailable: {0}")]
    DependencyUnavailable(String),
}
