use crate::domain::errors::KeyServiceError;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum ApiKeyScope {
    InferenceWrite,
    InferenceRead,
    KeyManagement,
    BillingRead,
    FilesRead,
    FilesWrite,
    FilesDelete,
}

impl ApiKeyScope {
    pub fn parse(value: &str) -> Result<Self, KeyServiceError> {
        match value {
            "inference:write" => Ok(Self::InferenceWrite),
            "inference:read" => Ok(Self::InferenceRead),
            "keys:manage" => Ok(Self::KeyManagement),
            "billing:read" => Ok(Self::BillingRead),
            "files:read" => Ok(Self::FilesRead),
            "files:write" => Ok(Self::FilesWrite),
            "files:delete" => Ok(Self::FilesDelete),
            _ => Err(KeyServiceError::InvalidScope(value.to_string())),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::InferenceWrite => "inference:write",
            Self::InferenceRead => "inference:read",
            Self::KeyManagement => "keys:manage",
            Self::BillingRead => "billing:read",
            Self::FilesRead => "files:read",
            Self::FilesWrite => "files:write",
            Self::FilesDelete => "files:delete",
        }
    }

    pub fn is_file_scope(&self) -> bool {
        matches!(self, Self::FilesRead | Self::FilesWrite | Self::FilesDelete)
    }

    pub fn is_model_scope(&self) -> bool {
        matches!(self, Self::InferenceRead | Self::InferenceWrite)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitOverride {
    pub rpm: Option<i32>,
    pub tpm: Option<i32>,
    pub concurrent: Option<i32>,
}

impl RateLimitOverride {
    pub fn validate(&self) -> Result<(), KeyServiceError> {
        for value in [self.rpm, self.tpm, self.concurrent].into_iter().flatten() {
            if value <= 0 {
                return Err(KeyServiceError::Validation(
                    "rate limit overrides must be positive".into(),
                ));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKey {
    pub id: Uuid,
    pub user_id: Uuid,
    pub name: String,
    pub key_prefix: String,
    pub key_hash: String,
    pub scopes: Vec<ApiKeyScope>,
    pub allowed_ips: Vec<IpAddr>,
    pub rate_limit_override: RateLimitOverride,
    pub status: String,
    pub expires_at: Option<DateTime<Utc>>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub last_used_ip: Option<IpAddr>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub revoked_reason: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl ApiKey {
    pub fn is_valid_for(
        &self,
        source_ip: Option<IpAddr>,
        now: DateTime<Utc>,
    ) -> Result<(), KeyServiceError> {
        if self.status == "revoked" {
            return Err(KeyServiceError::ApiKeyRevoked);
        }
        if self.status != "active" {
            return Err(KeyServiceError::Validation("invalid api key status".into()));
        }
        if self.expires_at.is_some_and(|expires_at| now >= expires_at) {
            return Err(KeyServiceError::ApiKeyExpired);
        }
        if !self.allowed_ips.is_empty()
            && !source_ip.is_some_and(|ip| self.allowed_ips.contains(&ip))
        {
            return Err(KeyServiceError::IpNotAllowed);
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct GeneratedApiKey {
    pub plaintext: String,
    pub hash: String,
    pub prefix: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use std::net::Ipv4Addr;

    #[test]
    fn parse_valid_scopes() {
        assert_eq!(
            ApiKeyScope::parse("inference:write").unwrap(),
            ApiKeyScope::InferenceWrite
        );
        assert_eq!(
            ApiKeyScope::parse("inference:read").unwrap(),
            ApiKeyScope::InferenceRead
        );
        assert_eq!(
            ApiKeyScope::parse("keys:manage").unwrap(),
            ApiKeyScope::KeyManagement
        );
        assert_eq!(
            ApiKeyScope::parse("billing:read").unwrap(),
            ApiKeyScope::BillingRead
        );
    }

    #[test]
    fn parse_invalid_scope() {
        assert!(ApiKeyScope::parse("admin:all").is_err());
        assert!(ApiKeyScope::parse("").is_err());
    }

    #[test]
    fn scope_as_str_roundtrip() {
        let cases = [
            "inference:write",
            "inference:read",
            "keys:manage",
            "billing:read",
            "files:read",
            "files:write",
            "files:delete",
        ];
        for case in cases {
            let scope = ApiKeyScope::parse(case).unwrap();
            assert_eq!(scope.as_str(), case);
        }
    }

    #[test]
    fn rate_limit_override_validates_positive_values() {
        let valid = RateLimitOverride {
            rpm: Some(100),
            tpm: Some(1000),
            concurrent: Some(5),
        };
        assert!(valid.validate().is_ok());
    }

    #[test]
    fn rate_limit_override_rejects_zero_or_negative() {
        assert!(RateLimitOverride {
            rpm: Some(0),
            tpm: None,
            concurrent: None,
        }
        .validate()
        .is_err());
        assert!(RateLimitOverride {
            rpm: None,
            tpm: Some(-1),
            concurrent: None,
        }
        .validate()
        .is_err());
    }

    #[test]
    fn rate_limit_override_allows_all_none() {
        assert!(RateLimitOverride {
            rpm: None,
            tpm: None,
            concurrent: None,
        }
        .validate()
        .is_ok());
    }

    fn active_key() -> ApiKey {
        ApiKey {
            id: Uuid::now_v7(),
            user_id: Uuid::now_v7(),
            name: "test-key".into(),
            key_prefix: "ak_test".into(),
            key_hash: "hash".into(),
            scopes: vec![ApiKeyScope::InferenceWrite],
            allowed_ips: vec![],
            rate_limit_override: RateLimitOverride {
                rpm: None,
                tpm: None,
                concurrent: None,
            },
            status: "active".into(),
            expires_at: None,
            last_used_at: None,
            last_used_ip: None,
            revoked_at: None,
            revoked_reason: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn active_key_is_valid() {
        let key = active_key();
        assert!(key.is_valid_for(None, Utc::now()).is_ok());
    }

    #[test]
    fn revoked_key_is_rejected() {
        let mut key = active_key();
        key.status = "revoked".into();
        assert!(key.is_valid_for(None, Utc::now()).is_err());
    }

    #[test]
    fn expired_key_is_rejected() {
        let mut key = active_key();
        key.expires_at = Some(Utc::now() - Duration::hours(1));
        assert!(key.is_valid_for(None, Utc::now()).is_err());
    }

    #[test]
    fn future_expiry_is_valid() {
        let mut key = active_key();
        key.expires_at = Some(Utc::now() + Duration::hours(1));
        assert!(key.is_valid_for(None, Utc::now()).is_ok());
    }

    #[test]
    fn ip_check_allows_when_allowed_ips_empty() {
        let key = active_key();
        assert!(key
            .is_valid_for(Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))), Utc::now())
            .is_ok());
    }

    #[test]
    fn ip_check_rejects_unlisted_ip() {
        let mut key = active_key();
        key.allowed_ips = vec![IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))];
        assert!(key
            .is_valid_for(Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1))), Utc::now())
            .is_err());
    }

    #[test]
    fn ip_check_accepts_listed_ip() {
        let mut key = active_key();
        key.allowed_ips = vec![IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))];
        assert!(key
            .is_valid_for(Some(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1))), Utc::now())
            .is_ok());
    }
}
