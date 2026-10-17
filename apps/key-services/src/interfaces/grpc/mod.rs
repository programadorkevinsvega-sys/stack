use crate::application::commands::{
    CreateApiKey, CreateApiKeyInput, RevokeApiKey, RotateApiKey, UpdateKeyMetadata,
};
use crate::application::ports::ApiKeyCache;
use crate::application::queries::{GetApiKey, ListApiKeys, ValidateKey};
use crate::domain::entities::{ApiKey, ApiKeyScope, RateLimitOverride};
use crate::domain::errors::KeyServiceError;
use chrono::{DateTime, Utc};
use proto::key_validation_v1::key_management_service_server::KeyManagementService;
use proto::key_validation_v1::key_validation_service_server::KeyValidationService;
use proto::key_validation_v1::{
    CreateKeyRequest, CreateKeyResponse, GetKeyRequest, GetKeyResponse,
    InvalidateUserKeyCacheRequest, KeyMetadata, ListKeysRequest, ListKeysResponse,
    RevokeKeyCacheRequest, RevokeKeyCacheResponse, RevokeKeyRequest, RevokeKeyResponse,
    RotateKeyRequest, RotateKeyResponse, UpdateKeyMetadataRequest, UpdateKeyMetadataResponse,
    ValidateKeyRequest, ValidateKeyResponse,
};
use std::net::IpAddr;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use uuid::Uuid;
pub struct KeyGrpcService {
    pub validate_key: ValidateKey,
    pub cache: Arc<dyn ApiKeyCache>,
}

pub struct KeyManagementGrpcService {
    pub create: CreateApiKey,
    pub revoke: RevokeApiKey,
    pub rotate: RotateApiKey,
    pub update: UpdateKeyMetadata,
    pub list: ListApiKeys,
    pub get: GetApiKey,
}
fn uuid(raw: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw).map_err(|_| Status::invalid_argument(format!("invalid {field}")))
}
fn metadata(key: ApiKey) -> KeyMetadata {
    KeyMetadata {
        id: key.id.to_string(),
        user_id: key.user_id.to_string(),
        name: key.name,
        key_prefix: key.key_prefix,
        scopes: key.scopes.iter().map(|v| v.as_str().into()).collect(),
        allowed_ips: key.allowed_ips.iter().map(ToString::to_string).collect(),
        status: key.status,
        rate_limit_override_rpm: key.rate_limit_override.rpm,
        rate_limit_override_tpm: key.rate_limit_override.tpm,
        max_concurrent_requests: key.rate_limit_override.concurrent,
        expires_at: key.expires_at.map(|v| v.to_rfc3339()),
        last_used_at: key.last_used_at.map(|v| v.to_rfc3339()),
        last_used_ip: key.last_used_ip.map(|v| v.to_string()),
        revoked_at: key.revoked_at.map(|v| v.to_rfc3339()),
        revoked_reason: key.revoked_reason,
        created_at: key.created_at.to_rfc3339(),
    }
}
fn scopes(values: Vec<String>) -> Result<Vec<ApiKeyScope>, Status> {
    values
        .iter()
        .map(|v| ApiKeyScope::parse(v).map_err(|e| Status::invalid_argument(e.to_string())))
        .collect()
}
fn ips(values: Vec<String>) -> Result<Vec<IpAddr>, Status> {
    values
        .iter()
        .map(|v| {
            v.parse()
                .map_err(|_| Status::invalid_argument("invalid allowed_ips"))
        })
        .collect()
}
fn overrides(rpm: Option<i32>, tpm: Option<i32>, concurrent: Option<i32>) -> RateLimitOverride {
    RateLimitOverride {
        rpm,
        tpm,
        concurrent,
    }
}
#[tonic::async_trait]
impl KeyManagementService for KeyManagementGrpcService {
    async fn create_key(
        &self,
        request: Request<CreateKeyRequest>,
    ) -> Result<Response<CreateKeyResponse>, Status> {
        let r = request.into_inner();
        let expires_at = r
            .expires_at
            .map(|v| {
                DateTime::parse_from_rfc3339(&v)
                    .map(|d| d.with_timezone(&Utc))
                    .map_err(|_| Status::invalid_argument("invalid expires_at"))
            })
            .transpose()?;
        let (key, plaintext) = self
            .create
            .execute(CreateApiKeyInput {
                user_id: uuid(&r.user_id, "user_id")?,
                name: r.name,
                scopes: scopes(r.scopes)?,
                allowed_ips: ips(r.allowed_ips)?,
                rate_limit_override: overrides(
                    r.rate_limit_override_rpm,
                    r.rate_limit_override_tpm,
                    r.max_concurrent_requests,
                ),
                expires_at,
            })
            .await
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        Ok(Response::new(CreateKeyResponse {
            key: Some(metadata(key)),
            plaintext_api_key: plaintext,
        }))
    }
    async fn revoke_key(
        &self,
        request: Request<RevokeKeyRequest>,
    ) -> Result<Response<RevokeKeyResponse>, Status> {
        let r = request.into_inner();
        self.revoke
            .execute(
                uuid(&r.key_id, "key_id")?,
                uuid(&r.user_id, "user_id")?,
                uuid(&r.revoked_by, "revoked_by")?,
                r.reason,
            )
            .await
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
        Ok(Response::new(RevokeKeyResponse { success: true }))
    }
    async fn rotate_key(
        &self,
        request: Request<RotateKeyRequest>,
    ) -> Result<Response<RotateKeyResponse>, Status> {
        let r = request.into_inner();
        let (key, plaintext) = self
            .rotate
            .execute(
                uuid(&r.key_id, "key_id")?,
                uuid(&r.user_id, "user_id")?,
                uuid(&r.revoked_by, "revoked_by")?,
                r.reason,
                if r.scopes.is_empty() {
                    None
                } else {
                    Some(scopes(r.scopes)?)
                },
            )
            .await
            .map_err(|e| Status::failed_precondition(e.to_string()))?;
        Ok(Response::new(RotateKeyResponse {
            key: Some(metadata(key)),
            plaintext_api_key: plaintext,
        }))
    }
    async fn update_key_metadata(
        &self,
        request: Request<UpdateKeyMetadataRequest>,
    ) -> Result<Response<UpdateKeyMetadataResponse>, Status> {
        let r = request.into_inner();
        let key = self
            .update
            .execute(
                uuid(&r.key_id, "key_id")?,
                uuid(&r.user_id, "user_id")?,
                r.name,
                ips(r.allowed_ips)?,
                overrides(
                    r.rate_limit_override_rpm,
                    r.rate_limit_override_tpm,
                    r.max_concurrent_requests,
                ),
                if r.scopes.is_empty() {
                    None
                } else {
                    Some(scopes(r.scopes)?)
                },
            )
            .await
            .map_err(|e| Status::invalid_argument(e.to_string()))?;
        Ok(Response::new(UpdateKeyMetadataResponse {
            key: Some(metadata(key)),
        }))
    }
    async fn list_keys(
        &self,
        request: Request<ListKeysRequest>,
    ) -> Result<Response<ListKeysResponse>, Status> {
        let keys = self
            .list
            .execute(uuid(&request.into_inner().user_id, "user_id")?)
            .await
            .map_err(|e| Status::internal(e.to_string()))?;
        Ok(Response::new(ListKeysResponse {
            keys: keys.into_iter().map(metadata).collect(),
        }))
    }
    async fn get_key(
        &self,
        request: Request<GetKeyRequest>,
    ) -> Result<Response<GetKeyResponse>, Status> {
        let r = request.into_inner();
        let key = self
            .get
            .execute(uuid(&r.key_id, "key_id")?, uuid(&r.user_id, "user_id")?)
            .await
            .map_err(|e| Status::not_found(e.to_string()))?;
        Ok(Response::new(GetKeyResponse {
            key: Some(metadata(key)),
        }))
    }
}
#[tonic::async_trait]
impl KeyValidationService for KeyGrpcService {
    async fn validate_key(
        &self,
        request: Request<ValidateKeyRequest>,
    ) -> Result<Response<ValidateKeyResponse>, Status> {
        let request = request.into_inner();
        let raw = request.api_key;
        let source_ip = request
            .source_ip
            .map(|ip| {
                ip.parse()
                    .map_err(|_| Status::invalid_argument("invalid source_ip"))
            })
            .transpose()?;
        let key = self
            .validate_key
            .execute(&raw, source_ip)
            .await
            .map_err(|e| match e {
                KeyServiceError::ApiKeyNotFound
                | KeyServiceError::ApiKeyRevoked
                | KeyServiceError::ApiKeyExpired
                | KeyServiceError::InvalidScope(_)
                | KeyServiceError::IpNotAllowed => Status::unauthenticated(e.to_string()),
                KeyServiceError::Validation(_) => Status::invalid_argument(e.to_string()),
                KeyServiceError::SubscriptionRequired => Status::failed_precondition(e.to_string()),
                KeyServiceError::MaxKeysPerUserReached | KeyServiceError::RateLimit(_) => {
                    Status::resource_exhausted(e.to_string())
                }
                _ => Status::unavailable("service temporarily unavailable"),
            })?;
        if let Some(required_scope) = request.required_scope {
            let required = ApiKeyScope::parse(&required_scope)
                .map_err(|e| Status::invalid_argument(e.to_string()))?;
            if !key.scopes.contains(&required) {
                return Err(Status::permission_denied("api key lacks required scope"));
            }
        }
        Ok(Response::new(ValidateKeyResponse {
            is_valid: true,
            api_key_id: key.id.to_string(),
            user_id: key.user_id.to_string(),
            status: key.status,
            scopes: key
                .scopes
                .iter()
                .map(|scope| scope.as_str().to_string())
                .collect(),
            rate_limit_override_rpm: key.rate_limit_override.rpm,
            rate_limit_override_tpm: key.rate_limit_override.tpm,
            max_concurrent_requests: key.rate_limit_override.concurrent,
            expires_at: key.expires_at.map(|value| value.to_rfc3339()),
        }))
    }
    async fn revoke_key_cache(
        &self,
        request: Request<RevokeKeyCacheRequest>,
    ) -> Result<Response<RevokeKeyCacheResponse>, Status> {
        self.cache
            .invalidate(&request.into_inner().key_hash)
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        Ok(Response::new(RevokeKeyCacheResponse { success: true }))
    }

    async fn invalidate_user_key_cache(
        &self,
        request: Request<InvalidateUserKeyCacheRequest>,
    ) -> Result<Response<RevokeKeyCacheResponse>, Status> {
        self.cache
            .invalidate_user(uuid(&request.into_inner().user_id, "user_id")?)
            .await
            .map_err(|e| Status::unavailable(e.to_string()))?;
        Ok(Response::new(RevokeKeyCacheResponse { success: true }))
    }
}
