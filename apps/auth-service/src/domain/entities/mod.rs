use chrono::{DateTime, Utc};
use sqlx::FromRow;
use uuid::Uuid;

use super::errors::AuthError;

#[derive(Debug, Clone, FromRow)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub password_hash: String,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub email_verified_at: Option<DateTime<Utc>>,
    pub mfa_enabled: bool,
    pub mfa_secret_encrypted: Option<Vec<u8>>,
    pub status: String,
    pub role: String,
    pub failed_login_count: i16,
    pub locked_until: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub last_login_ip: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl User {
    pub fn new(
        email: String,
        password_hash: String,
        display_name: Option<String>,
    ) -> Result<Self, AuthError> {
        if email.trim().is_empty() {
            return Err(AuthError::Validation("email must not be empty".into()));
        }
        if password_hash.is_empty() {
            return Err(AuthError::Validation(
                "password_hash must not be empty".into(),
            ));
        }
        let now = Utc::now();
        Ok(Self {
            id: Uuid::now_v7(),
            email,
            password_hash,
            display_name,
            avatar_url: None,
            email_verified_at: None,
            mfa_enabled: false,
            mfa_secret_encrypted: None,
            status: "active".into(),
            role: "user".into(),
            failed_login_count: 0,
            locked_until: None,
            last_login_at: None,
            last_login_ip: None,
            created_at: now,
            updated_at: now,
        })
    }

    pub fn is_active(&self) -> bool {
        self.status == "active"
    }
    pub fn is_locked(&self) -> bool {
        if let Some(locked_until) = self.locked_until {
            Utc::now() < locked_until
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct RefreshToken {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub expires_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl RefreshToken {
    pub fn new(
        user_id: Uuid,
        token_hash: String,
        expires_at: DateTime<Utc>,
        ip: Option<String>,
        ua: Option<String>,
    ) -> Result<Self, AuthError> {
        if token_hash.is_empty() {
            return Err(AuthError::Validation("token_hash must not be empty".into()));
        }
        if expires_at <= Utc::now() {
            return Err(AuthError::Validation(
                "expires_at must be in the future".into(),
            ));
        }
        Ok(Self {
            id: Uuid::now_v7(),
            user_id,
            token_hash,
            user_agent: ua,
            ip_address: ip,
            expires_at,
            revoked_at: None,
            created_at: Utc::now(),
        })
    }
}

#[derive(Debug, Clone, FromRow)]
pub struct PasswordReset {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl PasswordReset {
    pub fn new(user_id: Uuid, token_hash: String) -> Result<Self, AuthError> {
        if token_hash.is_empty() {
            return Err(AuthError::Validation("token_hash must not be empty".into()));
        }
        Ok(Self {
            id: Uuid::now_v7(),
            user_id,
            token_hash,
            expires_at: Utc::now() + chrono::Duration::minutes(15),
            used_at: None,
            created_at: Utc::now(),
        })
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TokenClaims {
    pub sub: Uuid,
    pub session_id: Uuid,
    pub email: String,
    pub exp: u64,
    pub iat: u64,
}

#[derive(Debug, Clone, FromRow)]
pub struct EmailVerification {
    pub id: Uuid,
    pub user_id: Uuid,
    pub token_hash: String,
    pub expires_at: DateTime<Utc>,
    pub verified_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

impl EmailVerification {
    pub fn new(user_id: Uuid, token_hash: String, ttl_minutes: i64) -> Result<Self, AuthError> {
        if token_hash.is_empty() {
            return Err(AuthError::Validation("token_hash must not be empty".into()));
        }
        if ttl_minutes <= 0 {
            return Err(AuthError::Validation("ttl_minutes must be positive".into()));
        }
        let expires_at = Utc::now() + chrono::Duration::minutes(ttl_minutes);
        Ok(Self {
            id: Uuid::now_v7(),
            user_id,
            token_hash,
            expires_at,
            verified_at: None,
            created_at: Utc::now(),
        })
    }
}
