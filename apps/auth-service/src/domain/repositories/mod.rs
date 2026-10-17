use super::entities::{EmailVerification, PasswordReset, RefreshToken, User};
use super::errors::AuthError;
use async_trait::async_trait;
use uuid::Uuid;

#[derive(Debug, Clone)]
pub struct LockResult {
    pub was_locked: bool,
    pub new_failed_count: i32,
    pub locked_until: Option<chrono::DateTime<chrono::Utc>>,
}

#[async_trait]
pub trait UserRepository: Send + Sync {
    async fn save(&self, user: &User) -> Result<(), AuthError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<User>, AuthError>;
    async fn find_by_email(&self, email: &str) -> Result<Option<User>, AuthError>;
    async fn find_all(
        &self,
        user_id: Option<Uuid>,
        limit: u32,
        after_cursor: Option<String>,
    ) -> Result<(Vec<User>, Option<String>), AuthError>;
    async fn update(&self, user: &User) -> Result<(), AuthError>;
    async fn anonymize(&self, id: Uuid) -> Result<(), AuthError>;
    async fn increment_failed_login(&self, id: Uuid) -> Result<(), AuthError>;
    async fn increment_failed_login_and_maybe_lock(
        &self,
        id: Uuid,
        max_attempts: i32,
        lock_duration_minutes: i32,
    ) -> Result<LockResult, AuthError>;
    async fn reset_failed_login(&self, id: Uuid) -> Result<(), AuthError>;
    async fn lock_account(
        &self,
        id: Uuid,
        until: chrono::DateTime<chrono::Utc>,
    ) -> Result<(), AuthError>;
}

#[async_trait]
pub trait RefreshTokenRepository: Send + Sync {
    async fn save(&self, token: &RefreshToken) -> Result<(), AuthError>;
    async fn find_by_hash(&self, hash: &str) -> Result<Option<RefreshToken>, AuthError>;
    async fn revoke(&self, id: Uuid) -> Result<(), AuthError>;
    async fn revoke_all_for_user(&self, user_id: Uuid) -> Result<(), AuthError>;
}

#[async_trait]
pub trait PasswordResetRepository: Send + Sync {
    async fn save(&self, reset: &PasswordReset) -> Result<(), AuthError>;
    async fn find_by_hash(&self, hash: &str) -> Result<Option<PasswordReset>, AuthError>;
    async fn mark_used(&self, id: Uuid) -> Result<(), AuthError>;
}

#[async_trait]
pub trait EmailVerificationRepository: Send + Sync {
    async fn save(&self, verification: &EmailVerification) -> Result<(), AuthError>;
    async fn find_by_hash(&self, hash: &str) -> Result<Option<EmailVerification>, AuthError>;
    async fn mark_verified(&self, id: Uuid) -> Result<(), AuthError>;
    async fn revoke_pending_for_user(&self, user_id: Uuid) -> Result<(), AuthError>;
}
