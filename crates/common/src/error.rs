use thiserror::Error;

/// Errores de dominio base. Cada servicio extiende con sus propias variantes
/// usando `#[error(transparent)]` o mapeando desde/hacia estas variantes base.
#[derive(Debug, Error)]
pub enum DomainError {
    #[error("resource not found: {0}")]
    NotFound(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("forbidden: {0}")]
    Forbidden(String),

    #[error("conflict: {0}")]
    Conflict(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("rate limited: retry after {retry_after}s")]
    RateLimited { retry_after: u64 },

    #[error("payment required: {0}")]
    PaymentRequired(String),

    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    #[error("internal error: {0}")]
    Internal(String),
}

impl DomainError {
    /// Crea un error de recurso no encontrado con el tipo de recurso y su id.
    pub fn not_found(resource: &str, id: impl std::fmt::Display) -> Self {
        Self::NotFound(format!("{} with id {} not found", resource, id))
    }

    /// Crea un error de validación.
    pub fn validation(msg: impl Into<String>) -> Self {
        Self::Validation(msg.into())
    }

    /// Crea un error interno (solo para casos realmente inesperados).
    pub fn internal(msg: impl Into<String>) -> Self {
        Self::Internal(msg.into())
    }
}
