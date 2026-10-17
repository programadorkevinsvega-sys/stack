use crate::application::ports::{
    AuditActorType, AuditEntry, AuditLogger, EmailSender, LoginRateLimiter, PasswordHasher,
    SensitiveDataCrypto, SessionStore, TokenSigner, TotpService,
};
use crate::domain::entities::{EmailVerification, PasswordReset, RefreshToken, TokenClaims, User};
use crate::domain::errors::AuthError;
use crate::domain::repositories::{
    EmailVerificationRepository, PasswordResetRepository, RefreshTokenRepository, UserRepository,
};
use chrono::{Duration, Utc};
use serde_json::json;
use sha2::{Digest, Sha256};
use std::net::IpAddr;
use std::str;
use std::sync::Arc;
use tracing::{info, warn};
use uuid::Uuid;

fn normalize_email(email: &str) -> Result<String, AuthError> {
    let normalized = email.trim().to_ascii_lowercase();
    if normalized.is_empty() || normalized.len() > 254 {
        return Err(AuthError::Validation("email is required".into()));
    }

    let at_pos = normalized
        .find('@')
        .ok_or_else(|| AuthError::Validation("invalid email format".into()))?;
    let local = &normalized[..at_pos];
    let domain = &normalized[at_pos + 1..];

    if local.is_empty() || local.len() > 64 || domain.is_empty() || domain.len() > 255 {
        return Err(AuthError::Validation("invalid email format".into()));
    }
    if !domain.contains('.') || domain.starts_with('.') || domain.ends_with('.') {
        return Err(AuthError::Validation("invalid email domain".into()));
    }
    if local.contains("..") || domain.contains("..") {
        return Err(AuthError::Validation("invalid email format".into()));
    }

    Ok(normalized)
}

fn validate_password_strength(password: &str) -> Result<(), AuthError> {
    if password.len() < 12 {
        return Err(AuthError::PasswordTooWeak(
            "must have at least 12 characters".into(),
        ));
    }

    let has_upper = password.chars().any(|ch| ch.is_ascii_uppercase());
    let has_lower = password.chars().any(|ch| ch.is_ascii_lowercase());
    let has_digit = password.chars().any(|ch| ch.is_ascii_digit());
    let has_special = password.chars().any(|ch| !ch.is_ascii_alphanumeric());
    if !has_upper || !has_lower || !has_digit || !has_special {
        return Err(AuthError::PasswordTooWeak(
            "must include at least 1 uppercase, 1 lowercase, 1 digit, and 1 special character"
                .into(),
        ));
    }

    Ok(())
}

fn hash_token(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

fn hash_email_for_audit(email: &str) -> String {
    hex::encode(Sha256::digest(email.as_bytes()))
}

fn parse_ip(ip: &str) -> Result<IpAddr, AuthError> {
    ip.parse::<IpAddr>()
        .map_err(|e| AuthError::Validation(format!("invalid client_ip: {e}")))
}

fn normalize_code(code: &str) -> Option<String> {
    let normalized = code.trim().replace(' ', "");
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn decode_secret(secret_bytes: Vec<u8>) -> Result<String, AuthError> {
    let secret = str::from_utf8(&secret_bytes)
        .map_err(|_| AuthError::Validation("invalid stored MFA secret encoding".into()))?;
    Ok(secret.to_string())
}

async fn record_audit_best_effort(audit: &Arc<dyn AuditLogger>, entry: AuditEntry) {
    if let Err(error) = audit.record(entry).await {
        warn!(error = %error, "Failed to write audit log");
    }
}

pub struct RegisterUser {
    pub repo: Arc<dyn UserRepository>,
    pub hasher: Arc<dyn PasswordHasher>,
    pub email: Arc<dyn EmailSender>,
    pub verification_repo: Arc<dyn EmailVerificationRepository>,
    pub email_verification_ttl_minutes: i64,
}

impl RegisterUser {
    pub async fn execute(
        &self,
        email: String,
        password: String,
        display_name: Option<String>,
    ) -> Result<User, AuthError> {
        let email = normalize_email(&email)?;
        validate_password_strength(&password)?;

        if self.repo.find_by_email(&email).await?.is_some() {
            let _ = self.hasher.hash(&password).await;
            return Err(AuthError::EmailAlreadyExists(email));
        }

        let hash = self.hasher.hash(&password).await?;
        let user = User::new(email.clone(), hash, display_name)?;
        self.repo.save(&user).await?;

        let token = Uuid::new_v4().to_string();
        let token_hash = hash_token(&token);
        let verification =
            EmailVerification::new(user.id, token_hash, self.email_verification_ttl_minutes)?;

        self.verification_repo
            .revoke_pending_for_user(user.id)
            .await?;
        self.verification_repo.save(&verification).await?;

        if let Err(error) = self.email.send_verification(&email, &token).await {
            warn!(user_id = %user.id, email = %email, error = %error, "Failed to send verification email");
        }

        info!(user_id = %user.id, "User registered");
        Ok(user)
    }
}

pub struct VerifyEmail {
    pub user_repo: Arc<dyn UserRepository>,
    pub verification_repo: Arc<dyn EmailVerificationRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

impl VerifyEmail {
    pub async fn execute(&self, token: &str) -> Result<(), AuthError> {
        let normalized_token = token.trim();
        if normalized_token.is_empty() {
            return Err(AuthError::InvalidEmailVerificationToken);
        }

        let token_hash = hash_token(normalized_token);
        let verification = self
            .verification_repo
            .find_by_hash(&token_hash)
            .await?
            .ok_or(AuthError::InvalidEmailVerificationToken)?;

        if verification.verified_at.is_some() {
            return Ok(());
        }
        if Utc::now() > verification.expires_at {
            return Err(AuthError::EmailVerificationExpired);
        }

        let mut user = self
            .user_repo
            .find_by_id(verification.user_id)
            .await?
            .ok_or(AuthError::UserNotFound)?;

        if user.email_verified_at.is_some() {
            self.verification_repo
                .mark_verified(verification.id)
                .await?;
            return Ok(());
        }

        user.email_verified_at = Some(Utc::now());
        self.user_repo.update(&user).await?;
        self.verification_repo
            .mark_verified(verification.id)
            .await?;

        record_audit_best_effort(
            &self.audit,
            AuditEntry {
                actor_type: AuditActorType::User,
                actor_id: Some(user.id),
                action: "auth.verify_email.succeeded".to_string(),
                resource_type: "auth.user".to_string(),
                resource_id: Some(user.id),
                metadata: json!({}),
                ip_address: None,
            },
        )
        .await;

        info!(user_id = %user.id, "Email verified");
        Ok(())
    }
}

pub struct RequestEmailVerification {
    pub user_repo: Arc<dyn UserRepository>,
    pub verification_repo: Arc<dyn EmailVerificationRepository>,
    pub email: Arc<dyn EmailSender>,
    pub email_verification_ttl_minutes: i64,
}

impl RequestEmailVerification {
    /// Anti-enumeración: siempre responde OK aunque el email no exista.
    pub async fn execute(&self, email: &str) -> Result<(), AuthError> {
        let normalized_email = normalize_email(email)?;

        if let Some(user) = self.user_repo.find_by_email(&normalized_email).await? {
            if user.email_verified_at.is_some() {
                return Ok(());
            }

            let token = Uuid::new_v4().to_string();
            let token_hash = hash_token(&token);
            let verification =
                EmailVerification::new(user.id, token_hash, self.email_verification_ttl_minutes)?;

            self.verification_repo
                .revoke_pending_for_user(user.id)
                .await?;
            self.verification_repo.save(&verification).await?;

            if let Err(error) = self
                .email
                .send_verification(&normalized_email, &token)
                .await
            {
                warn!(
                    user_id = %user.id,
                    email = %normalized_email,
                    error = %error,
                    "Failed to resend verification email"
                );
            }
        }

        Ok(())
    }
}

pub struct Login {
    pub repo: Arc<dyn UserRepository>,
    pub hasher: Arc<dyn PasswordHasher>,
    pub signer: Arc<dyn TokenSigner>,
    pub refresh_repo: Arc<dyn RefreshTokenRepository>,
    pub sessions: Arc<dyn SessionStore>,
    pub session_ttl_seconds: u64,
    pub secret_crypto: Arc<dyn SensitiveDataCrypto>,
    pub totp: Arc<dyn TotpService>,
    pub login_limiter: Arc<dyn LoginRateLimiter>,
    pub audit: Arc<dyn AuditLogger>,
}

struct LoginAuditDetails<'a> {
    actor_type: AuditActorType,
    actor_id: Option<Uuid>,
    resource_id: Option<Uuid>,
    action: &'a str,
    ip_address: IpAddr,
    user_agent: Option<&'a str>,
    reason: Option<&'a str>,
}

impl Login {
    async fn record_login_audit(&self, email: &str, details: LoginAuditDetails<'_>) {
        record_audit_best_effort(
            &self.audit,
            AuditEntry {
                actor_type: details.actor_type,
                actor_id: details.actor_id,
                action: details.action.to_string(),
                resource_type: "auth.user".to_string(),
                resource_id: details.resource_id,
                metadata: json!({
                    "email_sha256": hash_email_for_audit(email),
                    "reason": details.reason,
                    "user_agent": details.user_agent,
                }),
                ip_address: Some(details.ip_address),
            },
        )
        .await;
    }

    pub async fn execute(
        &self,
        email: &str,
        password: &str,
        mfa_code: Option<&str>,
        ip: Option<String>,
        ua: Option<String>,
    ) -> Result<(String, String), AuthError> {
        let email = normalize_email(email)?;
        let normalized_ip = ip
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| AuthError::Validation("client_ip is required".into()))?;
        let ip_address = parse_ip(&normalized_ip)?;
        let normalized_ua = ua
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());

        if let Err(error) = self
            .login_limiter
            .ensure_allowed(&email, &normalized_ip)
            .await
        {
            if matches!(error, AuthError::RateLimited(_)) {
                self.record_login_audit(
                    &email,
                    LoginAuditDetails {
                        actor_type: AuditActorType::System,
                        actor_id: None,
                        resource_id: None,
                        action: "auth.login.rate_limited",
                        ip_address,
                        user_agent: normalized_ua.as_deref(),
                        reason: Some("too_many_attempts"),
                    },
                )
                .await;
            }
            return Err(error);
        }

        let user = if let Some(found) = self.repo.find_by_email(&email).await? {
            found
        } else {
            let _ = self
                .hasher
                .verify(
                    "$argon2id$v=19$m=65536,t=3,p=4$dummy_salt$dummy_hash",
                    password,
                )
                .await;
            self.login_limiter
                .register_failure(&email, &normalized_ip)
                .await?;
            self.record_login_audit(
                &email,
                LoginAuditDetails {
                    actor_type: AuditActorType::System,
                    actor_id: None,
                    resource_id: None,
                    action: "auth.login.failed",
                    ip_address,
                    user_agent: normalized_ua.as_deref(),
                    reason: Some("invalid_credentials"),
                },
            )
            .await;
            return Err(AuthError::InvalidCredentials);
        };

        if user.status == "disabled" {
            self.record_login_audit(
                &email,
                LoginAuditDetails {
                    actor_type: AuditActorType::User,
                    actor_id: Some(user.id),
                    resource_id: Some(user.id),
                    action: "auth.login.failed",
                    ip_address,
                    user_agent: normalized_ua.as_deref(),
                    reason: Some("account_disabled"),
                },
            )
            .await;
            return Err(AuthError::AccountDisabled);
        }
        if user.is_locked() {
            self.record_login_audit(
                &email,
                LoginAuditDetails {
                    actor_type: AuditActorType::User,
                    actor_id: Some(user.id),
                    resource_id: Some(user.id),
                    action: "auth.login.failed",
                    ip_address,
                    user_agent: normalized_ua.as_deref(),
                    reason: Some("account_locked"),
                },
            )
            .await;
            return Err(AuthError::AccountLocked(
                user.locked_until
                    .ok_or_else(|| AuthError::Validation("locked_until is missing".into()))?
                    .to_rfc3339(),
            ));
        }
        if !self.hasher.verify(&user.password_hash, password).await? {
            self.login_limiter
                .register_failure(&email, &normalized_ip)
                .await?;
            let lock_result = self
                .repo
                .increment_failed_login_and_maybe_lock(user.id, 5, 30)
                .await?;
            if lock_result.was_locked {
                warn!(user_id = %user.id, "Account locked after 5 failed attempts");
            }

            self.record_login_audit(
                &email,
                LoginAuditDetails {
                    actor_type: AuditActorType::User,
                    actor_id: Some(user.id),
                    resource_id: Some(user.id),
                    action: "auth.login.failed",
                    ip_address,
                    user_agent: normalized_ua.as_deref(),
                    reason: Some("invalid_credentials"),
                },
            )
            .await;
            return Err(AuthError::InvalidCredentials);
        }

        if user.email_verified_at.is_none() {
            self.record_login_audit(
                &email,
                LoginAuditDetails {
                    actor_type: AuditActorType::User,
                    actor_id: Some(user.id),
                    resource_id: Some(user.id),
                    action: "auth.login.failed",
                    ip_address,
                    user_agent: normalized_ua.as_deref(),
                    reason: Some("email_not_verified"),
                },
            )
            .await;
            return Err(AuthError::EmailNotVerified);
        }

        if user.mfa_enabled {
            let encrypted_secret = user
                .mfa_secret_encrypted
                .as_ref()
                .ok_or(AuthError::MfaNotEnabled)?;
            let decrypted_secret = self
                .secret_crypto
                .decrypt_for_user(user.id, encrypted_secret)
                .await?;
            let secret = decode_secret(decrypted_secret)?;
            let code = if let Some(code) = mfa_code.and_then(normalize_code) {
                code
            } else {
                self.record_login_audit(
                    &email,
                    LoginAuditDetails {
                        actor_type: AuditActorType::User,
                        actor_id: Some(user.id),
                        resource_id: Some(user.id),
                        action: "auth.login.failed",
                        ip_address,
                        user_agent: normalized_ua.as_deref(),
                        reason: Some("mfa_required"),
                    },
                )
                .await;
                return Err(AuthError::MfaRequired);
            };

            let is_valid = self.totp.verify_code(&secret, &code)?;
            if !is_valid {
                self.record_login_audit(
                    &email,
                    LoginAuditDetails {
                        actor_type: AuditActorType::User,
                        actor_id: Some(user.id),
                        resource_id: Some(user.id),
                        action: "auth.login.failed",
                        ip_address,
                        user_agent: normalized_ua.as_deref(),
                        reason: Some("invalid_mfa_code"),
                    },
                )
                .await;
                return Err(AuthError::InvalidMfaCode);
            }
        }

        self.repo.reset_failed_login(user.id).await?;
        if let Err(error) = self.login_limiter.reset(&email, &normalized_ip).await {
            warn!(
                user_id = %user.id,
                error = %error,
                "Failed to reset login rate-limit counter"
            );
        }

        let refresh_plain = Uuid::new_v4().to_string();
        let refresh_hash = hash_token(&refresh_plain);
        let rt = RefreshToken::new(
            user.id,
            refresh_hash,
            Utc::now() + Duration::days(30),
            Some(normalized_ip.clone()),
            normalized_ua.clone(),
        )?;
        self.refresh_repo.save(&rt).await?;
        if let Err(error) = self
            .sessions
            .create(
                rt.id,
                user.id,
                self.session_ttl_seconds,
                normalized_ua.as_deref(),
                Some(normalized_ip.as_str()),
            )
            .await
        {
            if let Err(revoke_error) = self.refresh_repo.revoke(rt.id).await {
                tracing::error!(
                    session_id = %rt.id,
                    error = %revoke_error,
                    "failed to revoke refresh token after session creation failure"
                );
            }
            return Err(error);
        }

        let claims = TokenClaims {
            sub: user.id,
            session_id: rt.id,
            email: user.email.clone(),
            iat: Utc::now().timestamp() as u64,
            exp: (Utc::now() + Duration::hours(1)).timestamp() as u64,
        };
        let access = self.signer.sign(&claims)?;

        self.record_login_audit(
            &email,
            LoginAuditDetails {
                actor_type: AuditActorType::User,
                actor_id: Some(user.id),
                resource_id: Some(user.id),
                action: "auth.login.succeeded",
                ip_address,
                user_agent: normalized_ua.as_deref(),
                reason: None,
            },
        )
        .await;

        info!(user_id = %user.id, "Login successful");
        Ok((access, refresh_plain))
    }
}

pub struct RefreshAccessToken {
    pub refresh_repo: Arc<dyn RefreshTokenRepository>,
    pub signer: Arc<dyn TokenSigner>,
    pub user_repo: Arc<dyn UserRepository>,
    pub sessions: Arc<dyn SessionStore>,
    pub session_ttl_seconds: u64,
}

impl RefreshAccessToken {
    pub async fn execute(&self, refresh_token: &str) -> Result<(String, String), AuthError> {
        let hash = hash_token(refresh_token);
        let rt = self
            .refresh_repo
            .find_by_hash(&hash)
            .await?
            .ok_or(AuthError::InvalidRefreshToken)?;
        if rt.revoked_at.is_some() {
            return Err(AuthError::RefreshTokenRevoked);
        }
        if Utc::now() > rt.expires_at {
            return Err(AuthError::InvalidRefreshToken);
        }

        self.sessions.revoke(rt.id).await?;
        self.refresh_repo.revoke(rt.id).await?;

        let user = self
            .user_repo
            .find_by_id(rt.user_id)
            .await?
            .ok_or(AuthError::UserNotFound)?;

        let new_plain = Uuid::new_v4().to_string();
        let new_hash = hash_token(&new_plain);
        let new_rt = RefreshToken::new(
            user.id,
            new_hash,
            Utc::now() + Duration::days(30),
            None,
            None,
        )?;
        self.refresh_repo.save(&new_rt).await?;
        if let Err(error) = self
            .sessions
            .create(new_rt.id, user.id, self.session_ttl_seconds, None, None)
            .await
        {
            if let Err(revoke_error) = self.refresh_repo.revoke(new_rt.id).await {
                tracing::error!(
                    session_id = %new_rt.id,
                    error = %revoke_error,
                    "failed to revoke rotated refresh token after session creation failure"
                );
            }
            return Err(error);
        }
        let claims = TokenClaims {
            sub: user.id,
            session_id: new_rt.id,
            email: user.email,
            iat: Utc::now().timestamp() as u64,
            exp: (Utc::now() + Duration::hours(1)).timestamp() as u64,
        };
        let access = self.signer.sign(&claims)?;
        Ok((access, new_plain))
    }
}

pub struct Logout {
    pub refresh_repo: Arc<dyn RefreshTokenRepository>,
    pub sessions: Arc<dyn SessionStore>,
}

pub struct LogoutAll {
    pub refresh_repo: Arc<dyn RefreshTokenRepository>,
    pub sessions: Arc<dyn SessionStore>,
}

impl LogoutAll {
    pub async fn execute(&self, user_id: Uuid) -> Result<(), AuthError> {
        self.sessions.revoke_all_for_user(user_id).await?;
        self.refresh_repo.revoke_all_for_user(user_id).await
    }
}

pub struct UpdateProfile {
    pub repo: Arc<dyn UserRepository>,
}

impl UpdateProfile {
    pub async fn execute(
        &self,
        user_id: Uuid,
        display_name: Option<String>,
        avatar_url: Option<String>,
    ) -> Result<User, AuthError> {
        let mut user = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or(AuthError::UserNotFound)?;

        user.display_name = display_name
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        user.avatar_url = avatar_url
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        self.repo.update(&user).await?;
        Ok(user)
    }
}

pub struct UnlockAccount {
    pub repo: Arc<dyn UserRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

impl UnlockAccount {
    pub async fn execute(&self, user_id: Uuid, admin_actor_id: Uuid) -> Result<(), AuthError> {
        let mut user = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or(AuthError::UserNotFound)?;
        // The admin "unlock/activate" operation also re-enables accounts
        // previously disabled by an administrator.
        user.status = "active".to_string();
        user.failed_login_count = 0;
        user.locked_until = None;
        self.repo.update(&user).await?;
        record_audit_best_effort(
            &self.audit,
            AuditEntry {
                actor_type: AuditActorType::Admin,
                actor_id: Some(admin_actor_id),
                action: "auth.user.unlocked".to_string(),
                resource_type: "auth.user".to_string(),
                resource_id: Some(user_id),
                metadata: json!({}),
                ip_address: None,
            },
        )
        .await;
        Ok(())
    }
}

pub struct DisableUser {
    pub repo: Arc<dyn UserRepository>,
    pub refresh_repo: Arc<dyn RefreshTokenRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

pub struct DeleteUser {
    pub repo: Arc<dyn UserRepository>,
    pub refresh_repo: Arc<dyn RefreshTokenRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

impl DeleteUser {
    pub async fn execute(&self, user_id: Uuid, admin_actor_id: Uuid) -> Result<(), AuthError> {
        self.repo
            .find_by_id(user_id)
            .await?
            .ok_or(AuthError::UserNotFound)?;
        self.repo.anonymize(user_id).await?;
        self.refresh_repo.revoke_all_for_user(user_id).await?;
        record_audit_best_effort(
            &self.audit,
            AuditEntry {
                actor_type: AuditActorType::Admin,
                actor_id: Some(admin_actor_id),
                action: "auth.user.deleted_anonymized".to_string(),
                resource_type: "auth.user".to_string(),
                resource_id: Some(user_id),
                metadata: json!({ "retained_related_records": true }),
                ip_address: None,
            },
        )
        .await;
        Ok(())
    }
}

impl DisableUser {
    pub async fn execute(&self, user_id: Uuid, admin_actor_id: Uuid) -> Result<(), AuthError> {
        let mut user = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or(AuthError::UserNotFound)?;
        user.status = "disabled".to_string();
        user.locked_until = None;
        self.repo.update(&user).await?;
        self.refresh_repo.revoke_all_for_user(user_id).await?;
        record_audit_best_effort(
            &self.audit,
            AuditEntry {
                actor_type: AuditActorType::Admin,
                actor_id: Some(admin_actor_id),
                action: "auth.user.disabled".to_string(),
                resource_type: "auth.user".to_string(),
                resource_id: Some(user_id),
                metadata: json!({}),
                ip_address: None,
            },
        )
        .await;
        Ok(())
    }
}

impl Logout {
    pub async fn execute(&self, refresh_token: &str) -> Result<(), AuthError> {
        let hash = hash_token(refresh_token);
        if let Some(rt) = self.refresh_repo.find_by_hash(&hash).await? {
            self.sessions.revoke(rt.id).await?;
            self.refresh_repo.revoke(rt.id).await?;
        }
        Ok(())
    }
}

pub struct RequestPasswordReset {
    pub repo: Arc<dyn UserRepository>,
    pub reset_repo: Arc<dyn PasswordResetRepository>,
    pub email: Arc<dyn EmailSender>,
}

impl RequestPasswordReset {
    /// Siempre devuelve OK aunque el email no exista (anti-enumeración).
    pub async fn execute(&self, email_str: &str) -> Result<(), AuthError> {
        let email = normalize_email(email_str)?;
        if let Some(user) = self.repo.find_by_email(&email).await? {
            let plain = Uuid::new_v4().to_string();
            let hash = hash_token(&plain);
            let reset = PasswordReset::new(user.id, hash)?;
            self.reset_repo.save(&reset).await?;
            if let Err(error) = self.email.send_password_reset(&user.email, &plain).await {
                warn!(user_id = %user.id, email = %user.email, error = %error, "Failed to send password reset email");
            }
        }
        Ok(())
    }
}

pub struct ResetPassword {
    pub reset_repo: Arc<dyn PasswordResetRepository>,
    pub user_repo: Arc<dyn UserRepository>,
    pub hasher: Arc<dyn PasswordHasher>,
    pub refresh_repo: Arc<dyn RefreshTokenRepository>,
}

impl ResetPassword {
    pub async fn execute(&self, reset_token: &str, new_password: &str) -> Result<(), AuthError> {
        validate_password_strength(new_password)?;
        let hash = hash_token(reset_token);
        let reset = self
            .reset_repo
            .find_by_hash(&hash)
            .await?
            .ok_or(AuthError::PasswordResetExpired)?;
        if reset.used_at.is_some() {
            return Err(AuthError::PasswordResetUsed);
        }
        if Utc::now() > reset.expires_at {
            return Err(AuthError::PasswordResetExpired);
        }
        self.reset_repo.mark_used(reset.id).await?;
        let new_hash = self.hasher.hash(new_password).await?;
        let mut user = self
            .user_repo
            .find_by_id(reset.user_id)
            .await?
            .ok_or(AuthError::UserNotFound)?;
        user.password_hash = new_hash;
        self.user_repo.update(&user).await?;
        self.refresh_repo.revoke_all_for_user(user.id).await?;
        info!(user_id = %user.id, "Password reset completed");
        Ok(())
    }
}

pub struct ChangePassword {
    pub repo: Arc<dyn UserRepository>,
    pub hasher: Arc<dyn PasswordHasher>,
    pub refresh_repo: Arc<dyn RefreshTokenRepository>,
}

impl ChangePassword {
    pub async fn execute(
        &self,
        user_id: Uuid,
        old_pw: &str,
        new_pw: &str,
    ) -> Result<(), AuthError> {
        validate_password_strength(new_pw)?;
        let user = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or(AuthError::UserNotFound)?;
        if !self.hasher.verify(&user.password_hash, old_pw).await? {
            return Err(AuthError::InvalidCredentials);
        }
        let new_hash = self.hasher.hash(new_pw).await?;
        let mut u = user;
        u.password_hash = new_hash;
        self.repo.update(&u).await?;
        self.refresh_repo.revoke_all_for_user(user_id).await?;
        Ok(())
    }
}

pub struct ManageMfa {
    pub repo: Arc<dyn UserRepository>,
    pub secret_crypto: Arc<dyn SensitiveDataCrypto>,
    pub totp: Arc<dyn TotpService>,
    pub refresh_repo: Arc<dyn RefreshTokenRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

impl ManageMfa {
    pub async fn enable(&self, user_id: Uuid, account_name: String) -> Result<String, AuthError> {
        let mut user = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or(AuthError::UserNotFound)?;

        if user.mfa_enabled {
            return Err(AuthError::MfaAlreadyEnabled);
        }

        let account = account_name.trim().to_string();
        if account.is_empty() {
            return Err(AuthError::Validation("account_name is required".into()));
        }
        if account.contains(':') {
            return Err(AuthError::Validation(
                "account_name must not contain ':'".into(),
            ));
        }

        let secret = self.totp.generate_secret()?;
        let otpauth_uri = self.totp.build_otpauth_uri(&secret, &account)?;
        let encrypted_secret = self
            .secret_crypto
            .encrypt_for_user(user.id, secret.as_bytes())
            .await?;

        user.mfa_secret_encrypted = Some(encrypted_secret);
        user.mfa_enabled = false;
        self.repo.update(&user).await?;

        record_audit_best_effort(
            &self.audit,
            AuditEntry {
                actor_type: AuditActorType::User,
                actor_id: Some(user.id),
                action: "auth.mfa.enable.started".to_string(),
                resource_type: "auth.user".to_string(),
                resource_id: Some(user.id),
                metadata: json!({}),
                ip_address: None,
            },
        )
        .await;

        Ok(otpauth_uri)
    }

    pub async fn confirm(&self, user_id: Uuid, code: &str) -> Result<(), AuthError> {
        let normalized_code = normalize_code(code).ok_or(AuthError::InvalidMfaCode)?;

        let mut user = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or(AuthError::UserNotFound)?;

        if user.mfa_enabled {
            return Err(AuthError::MfaAlreadyEnabled);
        }

        let encrypted_secret = user
            .mfa_secret_encrypted
            .as_ref()
            .ok_or(AuthError::MfaNotEnabled)?;
        let decrypted_secret = self
            .secret_crypto
            .decrypt_for_user(user.id, encrypted_secret)
            .await?;
        let secret = decode_secret(decrypted_secret)?;

        let valid = self.totp.verify_code(&secret, &normalized_code)?;
        if !valid {
            return Err(AuthError::InvalidMfaCode);
        }

        user.mfa_enabled = true;
        self.repo.update(&user).await?;

        record_audit_best_effort(
            &self.audit,
            AuditEntry {
                actor_type: AuditActorType::User,
                actor_id: Some(user.id),
                action: "auth.mfa.confirmed".to_string(),
                resource_type: "auth.user".to_string(),
                resource_id: Some(user.id),
                metadata: json!({}),
                ip_address: None,
            },
        )
        .await;

        info!(user_id = %user.id, "MFA enabled");
        Ok(())
    }

    pub async fn disable(&self, user_id: Uuid, code: &str) -> Result<(), AuthError> {
        let normalized_code = normalize_code(code).ok_or(AuthError::InvalidMfaCode)?;

        let mut user = self
            .repo
            .find_by_id(user_id)
            .await?
            .ok_or(AuthError::UserNotFound)?;

        if !user.mfa_enabled {
            return Err(AuthError::MfaNotEnabled);
        }

        let encrypted_secret = user
            .mfa_secret_encrypted
            .as_ref()
            .ok_or(AuthError::MfaNotEnabled)?;
        let decrypted_secret = self
            .secret_crypto
            .decrypt_for_user(user.id, encrypted_secret)
            .await?;
        let secret = decode_secret(decrypted_secret)?;

        let valid = self.totp.verify_code(&secret, &normalized_code)?;
        if !valid {
            return Err(AuthError::InvalidMfaCode);
        }

        user.mfa_enabled = false;
        user.mfa_secret_encrypted = None;
        self.repo.update(&user).await?;
        self.refresh_repo.revoke_all_for_user(user_id).await?;

        record_audit_best_effort(
            &self.audit,
            AuditEntry {
                actor_type: AuditActorType::User,
                actor_id: Some(user.id),
                action: "auth.mfa.disabled".to_string(),
                resource_type: "auth.user".to_string(),
                resource_id: Some(user.id),
                metadata: json!({}),
                ip_address: None,
            },
        )
        .await;

        info!(user_id = %user.id, "MFA disabled");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_email_trims_and_lowercases() {
        let result = normalize_email("  User@Example.COM  ").expect("valid email");
        assert_eq!(result, "user@example.com");
    }

    #[test]
    fn normalize_email_rejects_empty() {
        assert!(normalize_email("").is_err());
        assert!(normalize_email("   ").is_err());
    }

    #[test]
    fn normalize_email_rejects_missing_at() {
        assert!(normalize_email("userexample.com").is_err());
        assert!(normalize_email("@example.com").is_err());
        assert!(normalize_email("user@").is_err());
    }

    #[test]
    fn validate_password_rejects_short() {
        assert!(validate_password_strength("Ab1!").is_err());
        assert!(validate_password_strength("Abcde12!").is_err());
    }

    #[test]
    fn validate_password_rejects_no_special() {
        assert!(validate_password_strength("Abcdefgh1234").is_err());
    }

    #[test]
    fn validate_password_accepts_valid() {
        assert!(validate_password_strength("Abcdefgh1!@#").is_ok());
        assert!(validate_password_strength("P@SSWORD12345a").is_ok());
        assert!(validate_password_strength("a1B@cdefghijklmn").is_ok());
    }

    #[test]
    fn normalize_code_strips_spaces() {
        assert_eq!(normalize_code("123 456"), Some("123456".to_string()));
        assert_eq!(normalize_code("  789  "), Some("789".to_string()));
    }

    #[test]
    fn normalize_code_returns_none_for_empty() {
        assert_eq!(normalize_code(""), None);
        assert_eq!(normalize_code("   "), None);
    }

    #[test]
    fn decode_secret_valid_utf8() {
        let result = decode_secret(b"JBSWY3DPEHPK3PXP".to_vec()).expect("valid utf8");
        assert_eq!(result, "JBSWY3DPEHPK3PXP");
    }

    #[test]
    fn decode_secret_rejects_invalid_utf8() {
        let invalid = vec![0xff, 0xfe, 0xfd];
        assert!(decode_secret(invalid).is_err());
    }
}
