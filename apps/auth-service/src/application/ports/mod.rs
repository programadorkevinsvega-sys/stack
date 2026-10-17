use crate::domain::entities::TokenClaims;
use crate::domain::errors::AuthError;
use async_trait::async_trait;
use serde_json::Value;
use std::net::IpAddr;
use uuid::Uuid;

#[async_trait]
pub trait PasswordHasher: Send + Sync {
    async fn hash(&self, password: &str) -> Result<String, AuthError>;
    async fn verify(&self, hash: &str, password: &str) -> Result<bool, AuthError>;
}

pub trait TokenSigner: Send + Sync {
    fn sign(&self, claims: &TokenClaims) -> Result<String, AuthError>;
    fn verify(&self, token: &str) -> Result<TokenClaims, AuthError>;
}

#[async_trait]
pub trait SessionStore: Send + Sync {
    async fn create(
        &self,
        session_id: Uuid,
        user_id: Uuid,
        ttl_seconds: u64,
        user_agent: Option<&str>,
        ip_address: Option<&str>,
    ) -> Result<(), AuthError>;
    async fn is_active(&self, session_id: Uuid) -> Result<bool, AuthError>;
    async fn revoke(&self, session_id: Uuid) -> Result<(), AuthError>;
    async fn revoke_all_for_user(&self, user_id: Uuid) -> Result<(), AuthError>;
    async fn list_for_user(&self, user_id: Uuid) -> Result<Vec<ActiveSession>, AuthError>;
    async fn revoke_for_user(&self, user_id: Uuid, session_id: Uuid) -> Result<bool, AuthError>;
}

#[derive(Debug, Clone)]
pub struct ActiveSession {
    pub session_id: Uuid,
    pub user_agent: Option<String>,
    pub ip_address: Option<String>,
    pub created_at: String,
    pub last_seen_at: String,
}

#[async_trait]
pub trait SensitiveDataCrypto: Send + Sync {
    async fn encrypt_for_user(&self, user_id: Uuid, plaintext: &[u8])
        -> Result<Vec<u8>, AuthError>;
    async fn decrypt_for_user(
        &self,
        user_id: Uuid,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, AuthError>;
}

pub trait TotpService: Send + Sync {
    fn generate_secret(&self) -> Result<String, AuthError>;
    fn build_otpauth_uri(&self, secret: &str, account_name: &str) -> Result<String, AuthError>;
    fn verify_code(&self, secret: &str, code: &str) -> Result<bool, AuthError>;
}

#[derive(Debug, Clone)]
pub enum AuditActorType {
    User,
    Admin,
    System,
}

impl AuditActorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Admin => "admin",
            Self::System => "system",
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuditEntry {
    pub actor_type: AuditActorType,
    pub actor_id: Option<Uuid>,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<Uuid>,
    pub metadata: Value,
    pub ip_address: Option<IpAddr>,
}

#[async_trait]
pub trait AuditLogger: Send + Sync {
    async fn record(&self, entry: AuditEntry) -> Result<(), AuthError>;
}

#[async_trait]
pub trait LoginRateLimiter: Send + Sync {
    async fn ensure_allowed(&self, email: &str, ip: &str) -> Result<(), AuthError>;
    async fn register_failure(&self, email: &str, ip: &str) -> Result<(), AuthError>;
    async fn reset(&self, email: &str, ip: &str) -> Result<(), AuthError>;
}

#[async_trait]
pub trait EmailSender: Send + Sync {
    async fn send_verification(&self, email: &str, token: &str) -> Result<(), AuthError>;
    async fn send_password_reset(&self, email: &str, token: &str) -> Result<(), AuthError>;
}
