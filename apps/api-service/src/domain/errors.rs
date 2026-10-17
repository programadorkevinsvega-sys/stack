use thiserror::Error;

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("model not found: {model_id}")]
    ModelNotFound { model_id: String },

    #[error("provider not found: {provider_id}")]
    ProviderNotFound { provider_id: String },

    #[error("provider inactive: {provider_id}")]
    ProviderInactive { provider_id: String },

    #[error("model name already exists: {name}")]
    ModelNameAlreadyExists { name: String },

    #[error("provider name already exists: {name}")]
    ProviderNameAlreadyExists { name: String },

    #[error("no active pricing for model {model_id}")]
    NoActivePricing { model_id: String },

    #[error("model deprecated: {model_id}")]
    ModelDeprecated { model_id: String },

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("dependency unavailable: {0}")]
    DependencyUnavailable(String),

    #[error("configuration error: {0}")]
    Configuration(String),

    #[error("internal error: {0}")]
    Internal(String),

    #[error("invalid data: {0}")]
    Validation(String),
}
