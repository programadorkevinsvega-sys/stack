use crate::application::ports::{SessionStore, TokenSigner};
use crate::domain::entities::User;
use crate::domain::errors::AuthError;
use crate::domain::repositories::UserRepository;
use std::sync::Arc;
use uuid::Uuid;

pub struct GetUser {
    pub repo: Arc<dyn UserRepository>,
}

impl GetUser {
    pub async fn execute(&self, id: Uuid) -> Result<Option<User>, AuthError> {
        self.repo.find_by_id(id).await
    }
}

pub struct GetUserByEmail {
    pub repo: Arc<dyn UserRepository>,
}

pub struct ListUsers {
    pub repo: Arc<dyn UserRepository>,
}

pub struct ValidateAccessToken {
    pub repo: Arc<dyn UserRepository>,
    pub signer: Arc<dyn TokenSigner>,
    pub sessions: Arc<dyn SessionStore>,
}

impl ValidateAccessToken {
    pub async fn execute(&self, access_token: &str) -> Result<User, AuthError> {
        let claims = self.signer.verify(access_token)?;
        if !self.sessions.is_active(claims.session_id).await? {
            return Err(AuthError::InvalidCredentials);
        }
        let user = self
            .repo
            .find_by_id(claims.sub)
            .await?
            .ok_or(AuthError::UserNotFound)?;
        if user.status == "disabled" {
            return Err(AuthError::AccountDisabled);
        }
        if user.is_locked() {
            return Err(AuthError::AccountLocked(
                user.locked_until
                    .ok_or_else(|| AuthError::Validation("locked_until is missing".into()))?
                    .to_rfc3339(),
            ));
        }
        Ok(user)
    }
}

impl ListUsers {
    pub async fn execute(
        &self,
        user_id: Option<Uuid>,
        limit: u32,
        after_cursor: Option<String>,
    ) -> Result<(Vec<User>, Option<String>), AuthError> {
        if limit == 0 || limit > 100 {
            return Err(AuthError::Validation(
                "limit must be between 1 and 100".into(),
            ));
        }
        self.repo.find_all(user_id, limit, after_cursor).await
    }
}

impl GetUserByEmail {
    pub async fn execute(&self, email: &str) -> Result<Option<User>, AuthError> {
        self.repo.find_by_email(email).await
    }
}
