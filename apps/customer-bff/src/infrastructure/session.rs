use crate::domain::errors::BffError;

pub fn require_env(key: &str) -> Result<String, BffError> {
    std::env::var(key).map_err(|_| BffError::Internal(format!("{key} is required")))
}

pub fn optional_env(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|v| !v.is_empty())
}
