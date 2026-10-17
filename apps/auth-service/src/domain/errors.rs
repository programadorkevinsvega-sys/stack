use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("user not found")]
    UserNotFound,
    #[error("email already exists: {0}")]
    EmailAlreadyExists(String),
    #[error("invalid credentials")]
    InvalidCredentials,
    #[error("account locked until {0}")]
    AccountLocked(String),
    #[error("account disabled")]
    AccountDisabled,
    #[error("email not verified")]
    EmailNotVerified,
    #[error("invalid email verification token")]
    InvalidEmailVerificationToken,
    #[error("email verification token expired")]
    EmailVerificationExpired,
    #[error("email already verified")]
    EmailAlreadyVerified,
    #[error("rate limited: {0}")]
    RateLimited(String),
    #[error("dependency unavailable: {0}")]
    DependencyUnavailable(String),
    #[error("invalid refresh token")]
    InvalidRefreshToken,
    #[error("refresh token revoked")]
    RefreshTokenRevoked,
    #[error("password reset token expired")]
    PasswordResetExpired,
    #[error("password reset token already used")]
    PasswordResetUsed,
    #[error("MFA required")]
    MfaRequired,
    #[error("invalid MFA code")]
    InvalidMfaCode,
    #[error("MFA already enabled")]
    MfaAlreadyEnabled,
    #[error("MFA not enabled")]
    MfaNotEnabled,
    #[error("password too weak: {0}")]
    PasswordTooWeak(String),
    #[error("{0}")]
    Validation(String),
}
