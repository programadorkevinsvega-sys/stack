use axum::http::StatusCode;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GatewayError {
    #[error("authentication required")]
    AuthenticationRequired,
    #[error("invalid api key")]
    InvalidApiKey,
    #[error("api key is not active")]
    ApiKeyInactive,
    #[error("rate limit exceeded")]
    RateLimited,
    #[error("payment required")]
    PaymentRequired,
    #[error("model not available")]
    ModelNotAvailable,
    #[error("upstream unavailable: {0}")]
    UpstreamUnavailable(String),
    #[error("invalid request: {0}")]
    InvalidRequestBody(String),
    #[error("validation error: {0}")]
    Validation(String),
    #[error("internal error: {0}")]
    Internal(String),
}

#[derive(Debug, Serialize)]
pub struct OpenAiErrorEnvelope {
    pub error: OpenAiError,
}

#[derive(Debug, Serialize)]
pub struct OpenAiError {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    pub code: String,
    pub param: Option<String>,
}

impl GatewayError {
    pub fn status_code(&self) -> StatusCode {
        match self {
            Self::AuthenticationRequired | Self::InvalidApiKey => StatusCode::UNAUTHORIZED,
            Self::ApiKeyInactive => StatusCode::FORBIDDEN,
            Self::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            Self::PaymentRequired => StatusCode::PAYMENT_REQUIRED,
            Self::ModelNotAvailable => StatusCode::SERVICE_UNAVAILABLE,
            Self::InvalidRequestBody(_) | Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::UpstreamUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    pub fn envelope(&self) -> OpenAiErrorEnvelope {
        let code = match self {
            Self::AuthenticationRequired => "authentication_required",
            Self::InvalidApiKey => "invalid_api_key",
            Self::ApiKeyInactive => "api_key_inactive",
            Self::RateLimited => "rate_limit_exceeded",
            Self::PaymentRequired => "payment_required",
            Self::ModelNotAvailable => "model_not_available",
            Self::UpstreamUnavailable(_) => "upstream_unavailable",
            Self::InvalidRequestBody(_) | Self::Validation(_) => "invalid_request_error",
            Self::Internal(_) => "internal_error",
        };
        let message = match self {
            Self::Internal(_) => "an internal error occurred".to_string(),
            Self::UpstreamUnavailable(_) => "upstream service temporarily unavailable".to_string(),
            other => other.to_string(),
        };
        OpenAiErrorEnvelope {
            error: OpenAiError {
                message,
                error_type: "api_error".into(),
                code: code.into(),
                param: None,
            },
        }
    }
}
