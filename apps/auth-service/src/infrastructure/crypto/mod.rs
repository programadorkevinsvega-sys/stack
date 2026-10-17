use crate::application::ports::{
    EmailSender, PasswordHasher, SensitiveDataCrypto, TokenSigner, TotpService,
};
use crate::domain::entities::TokenClaims;
use crate::domain::errors::AuthError;
use argon2::{
    password_hash::{
        rand_core::OsRng, PasswordHash, PasswordHasher as _, PasswordVerifier, SaltString,
    },
    Argon2,
};
use async_trait::async_trait;
use aws_sdk_kms::primitives::Blob;
use chrono::Utc;
use common::derive_kid;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header as JwtHeader, Validation};
use reqwest::Client;
use serde::Serialize;
use totp_rs::{Algorithm, Secret, TOTP};
use uuid::Uuid;

/// Hasher real con Argon2id (m=65536, t=3, p=4).
pub struct Argon2Hasher;

impl Argon2Hasher {
    fn argon2() -> Argon2<'static> {
        Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            argon2::Params::new(65536, 3, 4, None).expect("valid argon2 params"),
        )
    }
}

#[async_trait]
impl PasswordHasher for Argon2Hasher {
    async fn hash(&self, password: &str) -> Result<String, AuthError> {
        let password = password.to_string();
        tokio::task::spawn_blocking(move || {
            let salt = SaltString::generate(&mut OsRng);
            let hash = Self::argon2()
                .hash_password(password.as_bytes(), &salt)
                .map_err(|e| AuthError::Validation(format!("hash error: {e}")))?;
            Ok::<String, AuthError>(hash.to_string())
        })
        .await
        .map_err(|e| AuthError::Validation(format!("blocking task panicked: {e}")))?
    }

    async fn verify(&self, hash: &str, password: &str) -> Result<bool, AuthError> {
        let hash = hash.to_string();
        let password = password.to_string();
        tokio::task::spawn_blocking(move || {
            let parsed = PasswordHash::new(&hash)
                .map_err(|e| AuthError::Validation(format!("invalid hash: {e}")))?;
            Ok(Self::argon2()
                .verify_password(password.as_bytes(), &parsed)
                .is_ok())
        })
        .await
        .map_err(|e| AuthError::Validation(format!("blocking task panicked: {e}")))?
    }
}

/// Firmador/verificador de JWT con HS256.
pub struct JwtSigner {
    secret: String,
}

impl JwtSigner {
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
        }
    }
}

impl TokenSigner for JwtSigner {
    fn sign(&self, claims: &TokenClaims) -> Result<String, AuthError> {
        let kid = derive_kid(self.secret.as_bytes());
        let header = JwtHeader {
            kid: Some(kid),
            ..Default::default()
        };
        encode(
            &header,
            claims,
            &EncodingKey::from_secret(self.secret.as_bytes()),
        )
        .map_err(|e| AuthError::Validation(format!("jwt sign: {e}")))
    }

    fn verify(&self, token: &str) -> Result<TokenClaims, AuthError> {
        let mut validation = Validation::default();
        validation.leeway = 60;
        validation.validate_exp = true;
        let data = decode::<TokenClaims>(
            token,
            &DecodingKey::from_secret(self.secret.as_bytes()),
            &validation,
        )
        .map_err(|_| AuthError::InvalidCredentials)?;

        if data.claims.exp < Utc::now().timestamp() as u64 {
            return Err(AuthError::InvalidCredentials);
        }
        Ok(data.claims)
    }
}

#[derive(Serialize)]
struct ResendEmailRequest {
    from: String,
    to: Vec<String>,
    subject: String,
    html: String,
}

/// Envío real de email usando Resend API.
pub struct ResendEmailSender {
    client: Client,
    api_key: String,
    from_email: String,
    app_base_url: String,
}

impl ResendEmailSender {
    pub fn new(
        api_key: String,
        from_email: String,
        app_base_url: String,
    ) -> Result<Self, AuthError> {
        if api_key.trim().is_empty() {
            return Err(AuthError::Validation("RESEND_API_KEY is required".into()));
        }
        if from_email.trim().is_empty() {
            return Err(AuthError::Validation(
                "RESEND_FROM_EMAIL is required".into(),
            ));
        }
        if app_base_url.trim().is_empty() {
            return Err(AuthError::Validation(
                "AUTH_APP_BASE_URL is required".into(),
            ));
        }

        Ok(Self {
            client: Client::new(),
            api_key,
            from_email,
            app_base_url,
        })
    }

    async fn send(&self, to_email: &str, subject: &str, html: &str) -> Result<(), AuthError> {
        let payload = ResendEmailRequest {
            from: self.from_email.clone(),
            to: vec![to_email.to_string()],
            subject: subject.to_string(),
            html: html.to_string(),
        };

        let response = self
            .client
            .post("https://api.resend.com/emails")
            .bearer_auth(&self.api_key)
            .json(&payload)
            .send()
            .await
            .map_err(|e| AuthError::Validation(format!("resend request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response
                .text()
                .await
                .unwrap_or_else(|_| "unable to read body".to_string());
            return Err(AuthError::Validation(format!(
                "resend returned {status}: {body}"
            )));
        }

        Ok(())
    }

    fn verify_url(&self, token: &str) -> String {
        format!(
            "{}/auth/verify-email?token={}",
            self.app_base_url.trim_end_matches('/'),
            token
        )
    }

    fn reset_url(&self, token: &str) -> String {
        format!(
            "{}/auth/reset-password?token={}",
            self.app_base_url.trim_end_matches('/'),
            token
        )
    }
}

#[async_trait]
impl EmailSender for ResendEmailSender {
    async fn send_verification(&self, email: &str, token: &str) -> Result<(), AuthError> {
        let verify_url = self.verify_url(token);
        let html = format!(
            "<p>Confirma tu correo.</p><p><a href=\"{verify_url}\">Verificar email</a></p>"
        );

        self.send(email, "Verifica tu cuenta", &html).await
    }

    async fn send_password_reset(&self, email: &str, token: &str) -> Result<(), AuthError> {
        let reset_url = self.reset_url(token);
        let html = format!(
            "<p>Restablece tu contraseña.</p><p><a href=\"{reset_url}\">Cambiar contraseña</a></p>"
        );

        self.send(email, "Restablece tu contraseña", &html).await
    }
}

pub struct AwsKmsSensitiveDataCrypto {
    client: aws_sdk_kms::Client,
    kms_key_id: String,
}

impl AwsKmsSensitiveDataCrypto {
    pub fn new(client: aws_sdk_kms::Client, kms_key_id: String) -> Result<Self, AuthError> {
        if kms_key_id.trim().is_empty() {
            return Err(AuthError::Validation(
                "AUTH_MFA_KMS_KEY_ID is required".into(),
            ));
        }

        Ok(Self { client, kms_key_id })
    }
}

#[async_trait]
impl SensitiveDataCrypto for AwsKmsSensitiveDataCrypto {
    async fn encrypt_for_user(
        &self,
        user_id: Uuid,
        plaintext: &[u8],
    ) -> Result<Vec<u8>, AuthError> {
        let output = self
            .client
            .encrypt()
            .key_id(&self.kms_key_id)
            .plaintext(Blob::new(plaintext.to_vec()))
            .encryption_context("service", "auth-service")
            .encryption_context("purpose", "mfa-secret")
            .encryption_context("user_id", user_id.to_string())
            .send()
            .await
            .map_err(|e| AuthError::Validation(format!("kms encrypt failed: {e}")))?;

        let blob = output
            .ciphertext_blob()
            .ok_or_else(|| AuthError::Validation("kms encrypt returned empty ciphertext".into()))?;

        Ok(blob.as_ref().to_vec())
    }

    async fn decrypt_for_user(
        &self,
        user_id: Uuid,
        ciphertext: &[u8],
    ) -> Result<Vec<u8>, AuthError> {
        let output = self
            .client
            .decrypt()
            .key_id(&self.kms_key_id)
            .ciphertext_blob(Blob::new(ciphertext.to_vec()))
            .encryption_context("service", "auth-service")
            .encryption_context("purpose", "mfa-secret")
            .encryption_context("user_id", user_id.to_string())
            .send()
            .await
            .map_err(|e| AuthError::Validation(format!("kms decrypt failed: {e}")))?;

        let plaintext = output
            .plaintext()
            .ok_or_else(|| AuthError::Validation("kms decrypt returned empty plaintext".into()))?;

        Ok(plaintext.as_ref().to_vec())
    }
}

pub struct TotpRsService {
    issuer: String,
}

impl TotpRsService {
    pub fn new(issuer: String) -> Result<Self, AuthError> {
        let issuer = issuer.trim().to_string();
        if issuer.is_empty() {
            return Err(AuthError::Validation("AUTH_MFA_ISSUER is required".into()));
        }
        if issuer.contains(':') {
            return Err(AuthError::Validation(
                "AUTH_MFA_ISSUER must not contain ':'".into(),
            ));
        }

        Ok(Self { issuer })
    }

    fn build_totp(&self, secret: &str, account_name: String) -> Result<TOTP, AuthError> {
        let secret_bytes = Secret::Encoded(secret.to_string())
            .to_bytes()
            .map_err(|e| AuthError::Validation(format!("invalid TOTP secret encoding: {e}")))?;

        TOTP::new(
            Algorithm::SHA1,
            6,
            1,
            30,
            secret_bytes,
            Some(self.issuer.clone()),
            account_name,
        )
        .map_err(|e| AuthError::Validation(format!("failed to build TOTP: {e}")))
    }
}

impl TotpService for TotpRsService {
    fn generate_secret(&self) -> Result<String, AuthError> {
        let secret = Secret::generate_secret().to_encoded();
        match secret {
            Secret::Encoded(value) => Ok(value),
            Secret::Raw(_) => Err(AuthError::Validation(
                "unexpected raw secret generated by totp-rs".into(),
            )),
        }
    }

    fn build_otpauth_uri(&self, secret: &str, account_name: &str) -> Result<String, AuthError> {
        let account = account_name.trim();
        if account.is_empty() {
            return Err(AuthError::Validation(
                "account_name is required for MFA".into(),
            ));
        }
        if account.contains(':') {
            return Err(AuthError::Validation(
                "account_name must not contain ':'".into(),
            ));
        }

        let totp = self.build_totp(secret, account.to_string())?;
        Ok(totp.get_url())
    }

    fn verify_code(&self, secret: &str, code: &str) -> Result<bool, AuthError> {
        if !code.chars().all(|c| c.is_ascii_digit()) {
            return Err(AuthError::InvalidMfaCode);
        }
        if code.len() != 6 {
            return Err(AuthError::InvalidMfaCode);
        }

        let totp = self.build_totp(secret, String::new())?;
        totp.check_current(code).map_err(|e| {
            AuthError::Validation(format!("failed to validate TOTP at current time: {e}"))
        })
    }
}
