use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::clock::Clock;

pub fn derive_kid(secret: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(secret);
    hex::encode(&hasher.finalize()[..4])
}

#[derive(Debug, Clone)]
pub struct KeyEntry {
    pub kid: String,
    secret: Vec<u8>,
}

impl KeyEntry {
    pub fn new(kid: impl Into<String>, secret: impl Into<Vec<u8>>) -> Self {
        let secret = secret.into();
        let kid = kid.into();
        Self {
            kid: if kid.is_empty() {
                derive_kid(&secret)
            } else {
                kid
            },
            secret,
        }
    }

    pub fn secret(&self) -> &[u8] {
        &self.secret
    }
}

#[derive(Debug, Clone)]
pub struct KeyStore {
    entries: Vec<KeyEntry>,
    active_kid: String,
}

impl KeyStore {
    pub fn new(entries: Vec<KeyEntry>, active_kid: impl Into<String>) -> Self {
        let active_kid = active_kid.into();
        let active_kid = if active_kid.is_empty() {
            entries.first().map(|e| e.kid.clone()).unwrap_or_default()
        } else {
            active_kid
        };
        Self {
            entries,
            active_kid,
        }
    }

    pub fn from_env() -> Result<Self, String> {
        let signing = std::env::var("SERVICE_JWT_SECRET")
            .or_else(|_| std::env::var("SERVICE_JWT_SIGNING_SECRET"))
            .map_err(|_| "SERVICE_JWT_SECRET is required".to_string())?;
        let kid = std::env::var("SERVICE_JWT_KID").unwrap_or_default();
        let mut entries = vec![KeyEntry::new(&kid, signing.as_bytes().to_vec())];
        let active_kid = entries[0].kid.clone();
        if let Ok(prev) = std::env::var("SERVICE_JWT_PREVIOUS_SECRETS") {
            for s in prev.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
                entries.push(KeyEntry::new("", s.as_bytes().to_vec()));
            }
        }
        if let Ok(prev) = std::env::var("SERVICE_JWT_PREVIOUS_SECRET") {
            if !prev.is_empty() && prev != signing {
                entries.push(KeyEntry::new("", prev.as_bytes().to_vec()));
            }
        }
        Ok(Self {
            entries,
            active_kid,
        })
    }

    pub fn current_signing_entry(&self) -> Option<&KeyEntry> {
        self.entries.iter().find(|e| e.kid == self.active_kid)
    }

    pub fn entry_by_kid(&self, kid: &str) -> Option<&KeyEntry> {
        self.entries.iter().find(|e| e.kid == kid)
    }

    pub fn entries(&self) -> &[KeyEntry] {
        &self.entries
    }

    pub fn active_kid(&self) -> &str {
        &self.active_kid
    }
}

/// Claims de un JWT de autenticación servicio-a-servicio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceJwtClaims {
    pub iss: String,
    pub sub: String,
    pub exp: u64,
    pub iat: u64,
    pub jti: String,
}

impl ServiceJwtClaims {
    pub const MAX_TTL_SECONDS: u64 = 3600;

    pub fn new(
        issuer: impl Into<String>,
        subject: impl Into<String>,
        ttl_seconds: u64,
    ) -> Result<Self, String> {
        if ttl_seconds > Self::MAX_TTL_SECONDS {
            return Err(format!(
                "ttl_seconds exceeds maximum of {}",
                Self::MAX_TTL_SECONDS
            ));
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| format!("system clock error: {e}"))?
            .as_secs();
        Ok(Self {
            iss: issuer.into(),
            sub: subject.into(),
            exp: now + ttl_seconds,
            iat: now,
            jti: uuid::Uuid::now_v7().to_string(),
        })
    }

    pub fn new_with_clock(
        clock: &dyn Clock,
        issuer: impl Into<String>,
        subject: impl Into<String>,
        ttl_seconds: u64,
    ) -> Result<Self, String> {
        if ttl_seconds > Self::MAX_TTL_SECONDS {
            return Err(format!(
                "ttl_seconds exceeds maximum of {}",
                Self::MAX_TTL_SECONDS
            ));
        }
        let now = clock.now().timestamp() as u64;
        Ok(Self {
            iss: issuer.into(),
            sub: subject.into(),
            exp: now + ttl_seconds,
            iat: now,
            jti: uuid::Uuid::now_v7().to_string(),
        })
    }

    pub fn sign(&self, secret: &[u8]) -> Result<String, String> {
        jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            self,
            &jsonwebtoken::EncodingKey::from_secret(secret),
        )
        .map_err(|e| format!("failed to sign service jwt: {e}"))
    }

    pub fn sign_with_kid(&self, kid: &str, secret: &[u8]) -> Result<String, String> {
        let header = jsonwebtoken::Header {
            kid: Some(kid.to_string()),
            ..Default::default()
        };
        jsonwebtoken::encode(
            &header,
            self,
            &jsonwebtoken::EncodingKey::from_secret(secret),
        )
        .map_err(|e| format!("failed to sign service jwt: {e}"))
    }

    pub fn sign_with_keystore(&self, keystore: &KeyStore) -> Result<String, String> {
        let entry = keystore
            .current_signing_entry()
            .ok_or_else(|| "no active signing key configured".to_string())?;
        self.sign_with_kid(&entry.kid, entry.secret())
    }
}

#[derive(Clone)]
pub struct ServiceJwtAuth {
    decoding_keys: Vec<jsonwebtoken::DecodingKey>,
    kid_map: std::collections::HashMap<String, usize>,
    validation: jsonwebtoken::Validation,
    allowed_issuers: HashSet<String>,
    allowed_subjects: HashSet<String>,
}

impl ServiceJwtAuth {
    pub fn new(
        secret: impl AsRef<[u8]>,
        allowed_issuers: HashSet<String>,
        allowed_subjects: HashSet<String>,
    ) -> Result<Self, String> {
        Self::with_secrets(&[secret], allowed_issuers, allowed_subjects)
    }

    pub fn with_secrets(
        secrets: &[impl AsRef<[u8]>],
        allowed_issuers: HashSet<String>,
        allowed_subjects: HashSet<String>,
    ) -> Result<Self, String> {
        let mut decoding_keys = Vec::new();
        let mut kid_map = std::collections::HashMap::new();
        for secret in secrets {
            let key = secret.as_ref();
            if key.len() < 32 {
                return Err("SERVICE_JWT_SECRET must have at least 32 characters".to_string());
            }
            let kid = derive_kid(key);
            kid_map.insert(kid, decoding_keys.len());
            decoding_keys.push(jsonwebtoken::DecodingKey::from_secret(key));
        }
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.validate_exp = true;
        validation.required_spec_claims = std::collections::HashSet::from([
            "exp".to_string(),
            "iat".to_string(),
            "jti".to_string(),
        ]);
        Ok(Self {
            decoding_keys,
            kid_map,
            validation,
            allowed_issuers,
            allowed_subjects,
        })
    }

    pub fn from_keystore(
        keystore: &KeyStore,
        allowed_issuers: HashSet<String>,
        allowed_subjects: HashSet<String>,
    ) -> Result<Self, String> {
        let mut decoding_keys = Vec::new();
        let mut kid_map = std::collections::HashMap::new();
        for entry in keystore.entries() {
            if entry.secret().len() < 32 {
                return Err(format!(
                    "key {} must have at least 32 characters",
                    entry.kid
                ));
            }
            kid_map.insert(entry.kid.clone(), decoding_keys.len());
            decoding_keys.push(jsonwebtoken::DecodingKey::from_secret(entry.secret()));
        }
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::HS256);
        validation.validate_exp = true;
        validation.required_spec_claims = std::collections::HashSet::from([
            "exp".to_string(),
            "iat".to_string(),
            "jti".to_string(),
        ]);
        Ok(Self {
            decoding_keys,
            kid_map,
            validation,
            allowed_issuers,
            allowed_subjects,
        })
    }

    pub fn verify(&self, token: &str) -> Result<ServiceJwtClaims, tonic::Status> {
        let header = jsonwebtoken::decode_header(token)
            .map_err(|_| tonic::Status::unauthenticated("invalid token header"))?;
        let preferred_idx = header
            .kid
            .as_ref()
            .and_then(|kid| self.kid_map.get(kid))
            .copied();
        let order: Vec<usize> = if let Some(idx) = preferred_idx {
            let mut v: Vec<usize> = (0..self.decoding_keys.len()).collect();
            v.swap(0, idx);
            v
        } else {
            (0..self.decoding_keys.len()).collect()
        };
        let mut last_err = None;
        for idx in order {
            match jsonwebtoken::decode::<ServiceJwtClaims>(
                token,
                &self.decoding_keys[idx],
                &self.validation,
            ) {
                Ok(data) => {
                    if !self.allowed_issuers.contains(&data.claims.iss) {
                        return Err(tonic::Status::permission_denied(
                            "service issuer is not allowed",
                        ));
                    }
                    if !self.allowed_subjects.contains(&data.claims.sub) {
                        return Err(tonic::Status::permission_denied(
                            "service subject is not allowed",
                        ));
                    }
                    if data.claims.jti.is_empty() {
                        return Err(tonic::Status::unauthenticated("service token missing jti"));
                    }
                    if data.claims.exp
                        <= SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                    {
                        return Err(tonic::Status::unauthenticated("service token expired"));
                    }
                    return Ok(data.claims);
                }
                Err(e) => {
                    last_err = Some(e);
                }
            }
        }
        Err(tonic::Status::unauthenticated(format!(
            "invalid service token: {:?}",
            last_err.map(|e| e.to_string())
        )))
    }

    pub fn intercept(
        &self,
        mut req: tonic::Request<()>,
    ) -> Result<tonic::Request<()>, tonic::Status> {
        let auth_header = req
            .metadata()
            .get("authorization")
            .ok_or_else(|| tonic::Status::unauthenticated("missing authorization header"))?
            .to_str()
            .map_err(|_| tonic::Status::unauthenticated("invalid authorization header"))?;
        let token = auth_header
            .strip_prefix("Bearer ")
            .ok_or_else(|| tonic::Status::unauthenticated("bearer token required"))?;
        let claims = self.verify(token)?;
        req.extensions_mut().insert(claims);
        Ok(req)
    }
}

pub fn csv_env(key: &str) -> Result<HashSet<String>, String> {
    let value = std::env::var(key).map_err(|_| format!("{key} is required"))?;
    Ok(value
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect())
}
