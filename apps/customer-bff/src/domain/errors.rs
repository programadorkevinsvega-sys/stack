use axum::http::StatusCode;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BffError {
    #[error("authentication required")]
    AuthenticationRequired,

    #[error("invalid credentials")]
    InvalidCredentials,

    #[error("session expired")]
    SessionExpired,

    #[error("validation: {0}")]
    Validation(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("upstream unavailable: {0}")]
    UpstreamUnavailable(String),

    #[error("internal: {0}")]
    Internal(String),

    #[error("forbidden")]
    Forbidden,

    #[error("CSRF token mismatch")]
    CsrfTokenMismatch,

    #[error("rate limit exceeded")]
    RateLimited,
}

impl BffError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::AuthenticationRequired | Self::InvalidCredentials | Self::SessionExpired => {
                StatusCode::UNAUTHORIZED
            }
            Self::Validation(_) => StatusCode::UNPROCESSABLE_ENTITY,
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::UpstreamUnavailable(_) => StatusCode::BAD_GATEWAY,
            Self::Forbidden | Self::CsrfTokenMismatch => StatusCode::FORBIDDEN,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn code(&self) -> &'static str {
        match self {
            Self::AuthenticationRequired => "AUTH_REQUIRED",
            Self::InvalidCredentials => "INVALID_CREDENTIALS",
            Self::SessionExpired => "SESSION_EXPIRED",
            Self::Validation(_) => "VALIDATION_ERROR",
            Self::NotFound(_) => "NOT_FOUND",
            Self::UpstreamUnavailable(_) => "UPSTREAM_UNAVAILABLE",
            Self::Forbidden => "FORBIDDEN",
            Self::CsrfTokenMismatch => "CSRF_MISMATCH",
            Self::RateLimited => "RATE_LIMITED",
            Self::Internal(_) => "INTERNAL_ERROR",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::http::StatusCode;

    #[test]
    fn status_code_authentication_required() {
        assert_eq!(
            BffError::AuthenticationRequired.status_code(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn status_code_invalid_credentials() {
        assert_eq!(
            BffError::InvalidCredentials.status_code(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn status_code_session_expired() {
        assert_eq!(
            BffError::SessionExpired.status_code(),
            StatusCode::UNAUTHORIZED
        );
    }

    #[test]
    fn status_code_validation() {
        assert_eq!(
            BffError::Validation("bad".into()).status_code(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn status_code_not_found() {
        assert_eq!(
            BffError::NotFound("x".into()).status_code(),
            StatusCode::NOT_FOUND
        );
    }

    #[test]
    fn status_code_upstream_unavailable() {
        assert_eq!(
            BffError::UpstreamUnavailable("x".into()).status_code(),
            StatusCode::BAD_GATEWAY
        );
    }

    #[test]
    fn status_code_forbidden() {
        assert_eq!(BffError::Forbidden.status_code(), StatusCode::FORBIDDEN);
    }

    #[test]
    fn status_code_csrf_mismatch() {
        assert_eq!(
            BffError::CsrfTokenMismatch.status_code(),
            StatusCode::FORBIDDEN
        );
    }

    #[test]
    fn status_code_rate_limited() {
        assert_eq!(
            BffError::RateLimited.status_code(),
            StatusCode::TOO_MANY_REQUESTS
        );
    }

    #[test]
    fn status_code_internal() {
        assert_eq!(
            BffError::Internal("x".into()).status_code(),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn code_authentication_required() {
        assert_eq!(BffError::AuthenticationRequired.code(), "AUTH_REQUIRED");
    }

    #[test]
    fn code_invalid_credentials() {
        assert_eq!(BffError::InvalidCredentials.code(), "INVALID_CREDENTIALS");
    }

    #[test]
    fn code_session_expired() {
        assert_eq!(BffError::SessionExpired.code(), "SESSION_EXPIRED");
    }

    #[test]
    fn code_validation() {
        assert_eq!(
            BffError::Validation("bad".into()).code(),
            "VALIDATION_ERROR"
        );
    }

    #[test]
    fn code_not_found() {
        assert_eq!(BffError::NotFound("x".into()).code(), "NOT_FOUND");
    }

    #[test]
    fn code_upstream_unavailable() {
        assert_eq!(
            BffError::UpstreamUnavailable("x".into()).code(),
            "UPSTREAM_UNAVAILABLE"
        );
    }

    #[test]
    fn code_forbidden() {
        assert_eq!(BffError::Forbidden.code(), "FORBIDDEN");
    }

    #[test]
    fn code_csrf_mismatch() {
        assert_eq!(BffError::CsrfTokenMismatch.code(), "CSRF_MISMATCH");
    }

    #[test]
    fn code_rate_limited() {
        assert_eq!(BffError::RateLimited.code(), "RATE_LIMITED");
    }

    #[test]
    fn code_internal() {
        assert_eq!(BffError::Internal("x".into()).code(), "INTERNAL_ERROR");
    }

    #[test]
    fn display_authentication_required() {
        assert_eq!(
            BffError::AuthenticationRequired.to_string(),
            "authentication required"
        );
    }

    #[test]
    fn display_invalid_credentials() {
        assert_eq!(
            BffError::InvalidCredentials.to_string(),
            "invalid credentials"
        );
    }

    #[test]
    fn display_session_expired() {
        assert_eq!(BffError::SessionExpired.to_string(), "session expired");
    }

    #[test]
    fn display_validation() {
        assert_eq!(
            BffError::Validation("bad".into()).to_string(),
            "validation: bad"
        );
    }

    #[test]
    fn display_not_found() {
        assert_eq!(BffError::NotFound("x".into()).to_string(), "not found: x");
    }

    #[test]
    fn display_upstream_unavailable() {
        assert_eq!(
            BffError::UpstreamUnavailable("x".into()).to_string(),
            "upstream unavailable: x"
        );
    }

    #[test]
    fn display_forbidden() {
        assert_eq!(BffError::Forbidden.to_string(), "forbidden");
    }

    #[test]
    fn display_csrf_mismatch() {
        assert_eq!(
            BffError::CsrfTokenMismatch.to_string(),
            "CSRF token mismatch"
        );
    }

    #[test]
    fn display_rate_limited() {
        assert_eq!(BffError::RateLimited.to_string(), "rate limit exceeded");
    }

    #[test]
    fn display_internal() {
        assert_eq!(BffError::Internal("x".into()).to_string(), "internal: x");
    }
}
