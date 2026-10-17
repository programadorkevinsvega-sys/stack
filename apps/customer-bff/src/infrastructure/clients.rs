use crate::domain::errors::BffError;
use common::{derive_kid, ServiceJwtClaims, ServiceTlsConfig};
use proto::auth_v1::auth_service_client::AuthServiceClient;
use proto::auth_v1::{
    ChangePasswordRequest, ConfirmMfaRequest, DisableMfaRequest, EnableMfaRequest, GetUserRequest,
    ListSessionsRequest, LoginRequest, LogoutAllRequest, LogoutRequest, RefreshRequest,
    RegisterRequest, RevokeSessionRequest, UpdateProfileRequest, ValidateAccessTokenRequest,
    VerifyEmailRequest,
};
use proto::billing_reservation_v1::billing_wallet_service_client::BillingWalletServiceClient;
use proto::billing_reservation_v1::GetWalletRequest;
use proto::catalog_v1::catalog_service_client::CatalogServiceClient;
use proto::catalog_v1::{GetModelRequest, ListModelsRequest};
use proto::file_v1::file_service_client::FileServiceClient;
use proto::file_v1::{
    ConfirmFileRequest, CreateDownloadUrlRequest, CreateFileRequest, CreateFolderRequest,
    CreateUploadUrlRequest, DeleteFileRequest, DeleteFileVectorsRequest, DeleteFolderRequest,
    GetStorageUsageRequest, GetVectorUsageRequest, ListFilesRequest, ListFoldersRequest,
    ReindexFileVectorsRequest, SearchVectorsRequest, UpdateFileRequest, UpdateFolderRequest,
};
use proto::invoice_v1::invoice_service_client::InvoiceServiceClient;
use proto::invoice_v1::{GetInvoiceRequest, ListInvoicesByUserRequest};
use proto::key_validation_v1::key_management_service_client::KeyManagementServiceClient;
use proto::key_validation_v1::{
    CreateKeyRequest, GetKeyRequest, ListKeysRequest, RevokeKeyRequest, RotateKeyRequest,
    UpdateKeyMetadataRequest,
};
use proto::payment_v1::payment_service_client::PaymentServiceClient;
use proto::payment_v1::{
    CreatePayPalPaymentTokenRequest, CreatePayPalVaultSetupTokenRequest, CreateTopUpRequest,
    ListPaymentMethodsRequest, SetDefaultPaymentMethodRequest,
};
use proto::plan_resolution_v1::subscription_management_service_client::SubscriptionManagementServiceClient;
use proto::plan_resolution_v1::{
    AcceptTermsRequest, CancelSubscriptionRequest, CreateSubscriptionRequest,
    GetActiveSubscriptionRequest, GetPlanRequest, ListActivePlansRequest,
};
use proto::usage_v1::usage_reporting_service_client::UsageReportingServiceClient;
use proto::usage_v1::{
    CurrentMonthUsageRequest, LatencyStatsByUserRequest, TimeRange, TopModelsByUserRequest,
};
use std::time::Duration;
use tonic::metadata::MetadataValue;
use tonic::transport::Channel;
use tonic::Request;
use uuid::Uuid;

const GRPC_MAX_RETRIES: u32 = 3;

fn default_usage_range() -> TimeRange {
    let end = chrono::Utc::now();
    let start = end - chrono::Duration::days(30);
    TimeRange {
        start: start.to_rfc3339(),
        end: end.to_rfc3339(),
    }
}

fn map_grpc_status(status: tonic::Status) -> BffError {
    match status.code() {
        tonic::Code::InvalidArgument | tonic::Code::FailedPrecondition => {
            BffError::Validation(status.message().to_owned())
        }
        tonic::Code::NotFound => BffError::NotFound(status.message().to_owned()),
        tonic::Code::PermissionDenied => BffError::Forbidden,
        tonic::Code::Unauthenticated => BffError::AuthenticationRequired,
        tonic::Code::ResourceExhausted => BffError::RateLimited,
        _ => BffError::UpstreamUnavailable(status.to_string()),
    }
}

#[derive(Clone)]
pub struct GrpcClientConfig {
    pub endpoint: String,
    pub jwt_secret: String,
    pub issuer: String,
    pub subject: String,
    pub timeout_secs: u64,
    pub tls_config: Option<ServiceTlsConfig>,
}

#[derive(Clone)]
pub struct FileGrpcClient {
    pub config: GrpcClientConfig,
}

impl FileGrpcClient {
    pub fn new(config: GrpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn create_file(
        &self,
        request: CreateFileRequest,
    ) -> Result<proto::file_v1::File, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                client
                    .create_file(self.config.auth(Request::new(request.clone()))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner()
                    .file
                    .ok_or_else(|| BffError::Internal("file-service returned no file".into()))
            })
            .await
    }

    pub async fn list_files(
        &self,
        user_id: Uuid,
        folder_id: Option<String>,
    ) -> Result<Vec<proto::file_v1::File>, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                Ok(client
                    .list_files(self.config.auth(Request::new(ListFilesRequest {
                        user_id: user_id.to_string(),
                        folder_id: folder_id.clone(),
                    }))?)
                    .await
                    .map_err(map_grpc_status)?
                    .into_inner()
                    .files)
            })
            .await
    }

    pub async fn create_upload_url(
        &self,
        user_id: Uuid,
        file_id: Uuid,
        expires_seconds: Option<i32>,
    ) -> Result<proto::file_v1::UploadUrlResponse, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                client
                    .create_upload_url(self.config.auth(Request::new(CreateUploadUrlRequest {
                        user_id: user_id.to_string(),
                        file_id: file_id.to_string(),
                        expires_seconds,
                    }))?)
                    .await
                    .map_err(map_grpc_status)
                    .map(|r| r.into_inner())
            })
            .await
    }

    pub async fn create_folder(
        &self,
        user_id: Uuid,
        name: String,
        parent_id: Option<String>,
        idempotency_key: String,
    ) -> Result<proto::file_v1::Folder, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                client
                    .create_folder(self.config.auth(Request::new(CreateFolderRequest {
                        user_id: user_id.to_string(),
                        parent_id: parent_id.clone(),
                        name: name.clone(),
                        idempotency_key: idempotency_key.clone(),
                    }))?)
                    .await
                    .map_err(map_grpc_status)?
                    .into_inner()
                    .folder
                    .ok_or_else(|| BffError::Internal("file-service returned no folder".into()))
            })
            .await
    }

    pub async fn list_folders(
        &self,
        user_id: Uuid,
        parent_id: Option<String>,
    ) -> Result<Vec<proto::file_v1::Folder>, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                Ok(client
                    .list_folders(self.config.auth(Request::new(ListFoldersRequest {
                        user_id: user_id.to_string(),
                        parent_id: parent_id.clone(),
                    }))?)
                    .await
                    .map_err(map_grpc_status)?
                    .into_inner()
                    .folders)
            })
            .await
    }

    pub async fn confirm_file(
        &self,
        user_id: Uuid,
        file_id: Uuid,
    ) -> Result<proto::file_v1::File, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                client
                    .confirm_file(self.config.auth(Request::new(ConfirmFileRequest {
                        user_id: user_id.to_string(),
                        file_id: file_id.to_string(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner()
                    .file
                    .ok_or_else(|| BffError::Internal("file-service returned no file".into()))
            })
            .await
    }

    pub async fn delete_file(
        &self,
        user_id: Uuid,
        file_id: Uuid,
        reason: String,
    ) -> Result<(), BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                client
                    .delete_file(self.config.auth(Request::new(DeleteFileRequest {
                        user_id: user_id.to_string(),
                        file_id: file_id.to_string(),
                        reason: reason.clone(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?;
                Ok(())
            })
            .await
    }

    pub async fn download_url(
        &self,
        user_id: Uuid,
        file_id: Uuid,
        expires_seconds: Option<i32>,
    ) -> Result<proto::file_v1::DownloadUrlResponse, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                client
                    .create_download_url(self.config.auth(Request::new(
                        CreateDownloadUrlRequest {
                            user_id: user_id.to_string(),
                            file_id: file_id.to_string(),
                            expires_seconds,
                        },
                    ))?)
                    .await
                    .map_err(map_grpc_status)
                    .map(|r| r.into_inner())
            })
            .await
    }

    pub async fn update_file(
        &self,
        user_id: Uuid,
        file_id: Uuid,
        name: Option<String>,
        folder_id: Option<String>,
    ) -> Result<proto::file_v1::File, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                client
                    .update_file(self.config.auth(Request::new(UpdateFileRequest {
                        user_id: user_id.to_string(),
                        file_id: file_id.to_string(),
                        name: name.clone(),
                        folder_id: folder_id.clone(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner()
                    .file
                    .ok_or_else(|| BffError::Internal("file-service returned no file".into()))
            })
            .await
    }

    pub async fn update_folder(
        &self,
        user_id: Uuid,
        folder_id: Uuid,
        name: Option<String>,
        parent_id: Option<String>,
    ) -> Result<proto::file_v1::Folder, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                client
                    .update_folder(self.config.auth(Request::new(UpdateFolderRequest {
                        user_id: user_id.to_string(),
                        folder_id: folder_id.to_string(),
                        name: name.clone(),
                        parent_id: parent_id.clone(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner()
                    .folder
                    .ok_or_else(|| BffError::Internal("file-service returned no folder".into()))
            })
            .await
    }

    pub async fn delete_folder(
        &self,
        user_id: Uuid,
        folder_id: Uuid,
        reason: String,
    ) -> Result<(), BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                client
                    .delete_folder(self.config.auth(Request::new(DeleteFolderRequest {
                        user_id: user_id.to_string(),
                        folder_id: folder_id.to_string(),
                        reason: reason.clone(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?;
                Ok(())
            })
            .await
    }

    pub async fn search_vectors(
        &self,
        user_id: Uuid,
        query: String,
        limit: i32,
        file_id: Option<String>,
    ) -> Result<Vec<proto::file_v1::VectorSearchHit>, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                Ok(client
                    .search_vectors(self.config.auth(Request::new(SearchVectorsRequest {
                        user_id: user_id.to_string(),
                        query: query.clone(),
                        limit,
                        file_id: file_id.clone(),
                    }))?)
                    .await
                    .map_err(map_grpc_status)?
                    .into_inner()
                    .hits)
            })
            .await
    }

    pub async fn vector_usage(
        &self,
        user_id: Uuid,
    ) -> Result<proto::file_v1::VectorUsageResponse, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                client
                    .get_vector_usage(self.config.auth(Request::new(GetVectorUsageRequest {
                        user_id: user_id.to_string(),
                    }))?)
                    .await
                    .map_err(map_grpc_status)
                    .map(|response| response.into_inner())
            })
            .await
    }

    pub async fn storage_usage(
        &self,
        user_id: Uuid,
    ) -> Result<proto::file_v1::StorageUsageResponse, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                client
                    .get_storage_usage(self.config.auth(Request::new(GetStorageUsageRequest {
                        user_id: user_id.to_string(),
                    }))?)
                    .await
                    .map_err(map_grpc_status)
                    .map(|response| response.into_inner())
            })
            .await
    }

    pub async fn delete_file_vectors(
        &self,
        user_id: Uuid,
        file_id: Uuid,
        reason: String,
    ) -> Result<i64, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                client
                    .delete_file_vectors(self.config.auth(Request::new(
                        DeleteFileVectorsRequest {
                            user_id: user_id.to_string(),
                            file_id: file_id.to_string(),
                            reason: reason.clone(),
                        },
                    ))?)
                    .await
                    .map_err(map_grpc_status)
                    .map(|response| response.into_inner().deleted_vectors)
            })
            .await
    }

    pub async fn reindex_file_vectors(&self, user_id: Uuid, file_id: Uuid) -> Result<(), BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                client
                    .reindex_file_vectors(self.config.auth(Request::new(
                        ReindexFileVectorsRequest {
                            user_id: user_id.to_string(),
                            file_id: file_id.to_string(),
                        },
                    ))?)
                    .await
                    .map_err(map_grpc_status)?;
                Ok(())
            })
            .await
    }
}

async fn connect_with_retry(
    endpoint: &str,
    timeout_secs: u64,
    tls_config: Option<&ServiceTlsConfig>,
) -> Result<Channel, BffError> {
    let ep = tonic::transport::Endpoint::from_shared(endpoint.to_string())
        .map_err(|e| BffError::Internal(e.to_string()))?
        .timeout(Duration::from_secs(timeout_secs));
    let ep = if let Some(tls_config) = tls_config {
        common::tls::configure_client_endpoint(ep, endpoint, tls_config)
            .await
            .map_err(|e| BffError::Internal(e.to_string()))?
    } else {
        ep
    };
    let mut last_error = None;
    for attempt in 0..=GRPC_MAX_RETRIES {
        match ep.connect().await {
            Ok(ch) => return Ok(ch),
            Err(e) => {
                tracing::warn!(%endpoint, attempt, error = %e, "gRPC connect attempt failed");
                if attempt < GRPC_MAX_RETRIES {
                    let delay = Duration::from_millis(50 * 2u64.pow(attempt));
                    tokio::time::sleep(delay).await;
                    last_error = Some(BffError::UpstreamUnavailable(e.to_string()));
                } else {
                    return Err(BffError::UpstreamUnavailable(e.to_string()));
                }
            }
        }
    }
    Err(last_error.unwrap_or(BffError::Internal("connect retry exhausted".into())))
}

impl GrpcClientConfig {
    pub fn retry_call<F, Fut, T>(
        &self,
        factory: F,
    ) -> impl std::future::Future<Output = Result<T, BffError>>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T, BffError>>,
    {
        let endpoint = self.endpoint.clone();
        let max_retries = 2u32;
        async move {
            let mut last_error = None;
            for attempt in 0..=max_retries {
                match factory().await {
                    Ok(val) => return Ok(val),
                    Err(BffError::UpstreamUnavailable(_)) if attempt < max_retries => {
                        tracing::warn!(%endpoint, attempt, "gRPC call retry");
                        let delay = Duration::from_millis(100 * 2u64.pow(attempt));
                        tokio::time::sleep(delay).await;
                        last_error = Some(BffError::Internal("retry exhausted".into()));
                    }
                    Err(e) => return Err(e),
                }
            }
            Err(last_error.unwrap_or(BffError::Internal("retry exhausted".into())))
        }
    }
}

impl GrpcClientConfig {
    pub fn token(&self) -> Result<String, BffError> {
        let claims = ServiceJwtClaims::new(&self.issuer, &self.subject, 60)
            .map_err(|e| BffError::Internal(e.to_string()))?;
        let kid = derive_kid(self.jwt_secret.as_bytes());
        claims
            .sign_with_kid(&kid, self.jwt_secret.as_bytes())
            .map_err(|e| BffError::Internal(e.to_string()))
    }

    pub async fn channel(&self) -> Result<Channel, BffError> {
        connect_with_retry(&self.endpoint, self.timeout_secs, self.tls_config.as_ref()).await
    }

    pub fn auth<T>(&self, mut request: Request<T>) -> Result<Request<T>, BffError> {
        let auth = format!("Bearer {}", self.token()?);
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(auth).map_err(|e| BffError::Internal(e.to_string()))?,
        );
        common::observability::inject_tonic_context(&mut request);
        Ok(request)
    }
}

pub struct LoginResult {
    pub access_token: String,
    pub refresh_token: String,
    pub user: proto::auth_v1::User,
}

pub struct ValidatedSession {
    pub user_id: Uuid,
    pub email: String,
}

#[derive(Clone)]
pub struct AuthGrpcClient {
    pub config: GrpcClientConfig,
}

impl AuthGrpcClient {
    pub fn new(config: GrpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn register(
        &self,
        email: String,
        password: String,
        display_name: Option<String>,
    ) -> Result<proto::auth_v1::User, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                let resp = client
                    .register(self.config.auth(Request::new(RegisterRequest {
                        email: email.clone(),
                        password: password.clone(),
                        display_name: display_name.clone(),
                    }))?)
                    .await
                    .map_err(map_grpc_status)?
                    .into_inner();
                resp.user
                    .ok_or(BffError::Internal("no user returned".into()))
            })
            .await
    }

    pub async fn login(
        &self,
        email: String,
        password: String,
        mfa_code: Option<String>,
        ip: Option<String>,
        user_agent: Option<String>,
    ) -> Result<LoginResult, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                let resp = client
                    .login(self.config.auth(Request::new(LoginRequest {
                        email: email.clone(),
                        password: password.clone(),
                        mfa_code: mfa_code.clone(),
                        client_ip: ip.clone(),
                        user_agent: user_agent.clone(),
                    }))?)
                    .await
                    .map_err(map_grpc_status)?
                    .into_inner();
                let user = self.get_user_from_token(&resp.access_token).await?;
                Ok(LoginResult {
                    access_token: resp.access_token,
                    refresh_token: resp.refresh_token,
                    user,
                })
            })
            .await
    }

    pub async fn refresh(&self, token: String) -> Result<LoginResult, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                let resp = client
                    .refresh(self.config.auth(Request::new(RefreshRequest {
                        refresh_token: token.clone(),
                    }))?)
                    .await
                    .map_err(map_grpc_status)?
                    .into_inner();
                let user = self.get_user_from_token(&resp.access_token).await?;
                Ok(LoginResult {
                    access_token: resp.access_token,
                    refresh_token: resp.refresh_token,
                    user,
                })
            })
            .await
    }

    pub async fn logout(&self, _user_id: Uuid, refresh_token: String) -> Result<(), BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                client
                    .logout(self.config.auth(Request::new(LogoutRequest {
                        refresh_token: refresh_token.clone(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?;
                Ok(())
            })
            .await
    }

    pub async fn logout_all(&self, user_id: Uuid) -> Result<(), BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                client
                    .logout_all(self.config.auth(Request::new(LogoutAllRequest {
                        user_id: user_id.to_string(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?;
                Ok(())
            })
            .await
    }

    pub async fn list_sessions(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<proto::auth_v1::ActiveSession>, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                let response = client
                    .list_sessions(self.config.auth(Request::new(ListSessionsRequest {
                        user_id: user_id.to_string(),
                    }))?)
                    .await
                    .map_err(map_grpc_status)?
                    .into_inner();
                Ok(response.sessions)
            })
            .await
    }

    pub async fn revoke_session(&self, user_id: Uuid, session_id: Uuid) -> Result<(), BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                client
                    .revoke_session(self.config.auth(Request::new(RevokeSessionRequest {
                        user_id: user_id.to_string(),
                        session_id: session_id.to_string(),
                    }))?)
                    .await
                    .map_err(map_grpc_status)?;
                Ok(())
            })
            .await
    }

    pub async fn get_user(&self, user_id: Uuid) -> Result<proto::auth_v1::User, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                let resp = client
                    .get_user(self.config.auth(Request::new(GetUserRequest {
                        user_id: user_id.to_string(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.user.ok_or(BffError::NotFound("user not found".into()))
            })
            .await
    }

    pub async fn validate_token(&self, token: &str) -> Result<ValidatedSession, BffError> {
        let owned = token.to_string();
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                let resp = client
                    .validate_access_token(self.config.auth(Request::new(
                        ValidateAccessTokenRequest {
                            access_token: owned.clone(),
                        },
                    ))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                let user = resp.user.ok_or(BffError::InvalidCredentials)?;
                let user_id = Uuid::parse_str(&user.id)
                    .map_err(|e| BffError::Internal(format!("invalid user id: {e}")))?;
                Ok(ValidatedSession {
                    user_id,
                    email: user.email,
                })
            })
            .await
    }

    async fn get_user_from_token(&self, token: &str) -> Result<proto::auth_v1::User, BffError> {
        let owned = token.to_string();
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                let resp = client
                    .validate_access_token(self.config.auth(Request::new(
                        ValidateAccessTokenRequest {
                            access_token: owned.clone(),
                        },
                    ))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.user
                    .ok_or(BffError::Internal("no user from token".into()))
            })
            .await
    }

    pub async fn change_password(
        &self,
        user_id: Uuid,
        old_password: String,
        new_password: String,
    ) -> Result<(), BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                client
                    .change_password(self.config.auth(Request::new(ChangePasswordRequest {
                        user_id: user_id.to_string(),
                        old_password: old_password.clone(),
                        new_password: new_password.clone(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?;
                Ok(())
            })
            .await
    }

    pub async fn update_profile(
        &self,
        user_id: Uuid,
        display_name: Option<String>,
        avatar_url: Option<String>,
    ) -> Result<proto::auth_v1::User, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                let resp = client
                    .update_profile(self.config.auth(Request::new(UpdateProfileRequest {
                        user_id: user_id.to_string(),
                        display_name: display_name.clone(),
                        avatar_url: avatar_url.clone(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.user
                    .ok_or(BffError::Internal("no user returned".into()))
            })
            .await
    }

    pub async fn verify_email(&self, token: String) -> Result<(), BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                client
                    .verify_email(self.config.auth(Request::new(VerifyEmailRequest {
                        token: token.clone(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?;
                Ok(())
            })
            .await
    }

    pub async fn enable_mfa(&self, user_id: Uuid) -> Result<String, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                let resp = client
                    .enable_mfa(self.config.auth(Request::new(EnableMfaRequest {
                        user_id: user_id.to_string(),
                        account_name: None,
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.otpauth_uri)
            })
            .await
    }

    pub async fn confirm_mfa(&self, user_id: Uuid, code: String) -> Result<(), BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                client
                    .confirm_mfa(self.config.auth(Request::new(ConfirmMfaRequest {
                        user_id: user_id.to_string(),
                        code: code.clone(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?;
                Ok(())
            })
            .await
    }

    pub async fn disable_mfa(&self, user_id: Uuid, code: String) -> Result<(), BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                client
                    .disable_mfa(self.config.auth(Request::new(DisableMfaRequest {
                        user_id: user_id.to_string(),
                        code: code.clone(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?;
                Ok(())
            })
            .await
    }
}

#[derive(Clone)]
pub struct KeyGrpcClient {
    pub config: GrpcClientConfig,
}

impl KeyGrpcClient {
    pub fn new(config: GrpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn list_keys(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<proto::key_validation_v1::KeyMetadata>, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = KeyManagementServiceClient::new(self.config.channel().await?);
                let resp = client
                    .list_keys(self.config.auth(Request::new(ListKeysRequest {
                        user_id: user_id.to_string(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.keys)
            })
            .await
    }

    pub async fn get_key(
        &self,
        key_id: Uuid,
        user_id: Uuid,
    ) -> Result<proto::key_validation_v1::KeyMetadata, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = KeyManagementServiceClient::new(self.config.channel().await?);
                let resp = client
                    .get_key(self.config.auth(Request::new(GetKeyRequest {
                        key_id: key_id.to_string(),
                        user_id: user_id.to_string(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.key.ok_or(BffError::NotFound("key not found".into()))
            })
            .await
    }

    pub async fn create_key(
        &self,
        user_id: Uuid,
        name: String,
        scopes: Vec<String>,
    ) -> Result<proto::key_validation_v1::CreateKeyResponse, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = KeyManagementServiceClient::new(self.config.channel().await?);
                let resp = client
                    .create_key(self.config.auth(Request::new(CreateKeyRequest {
                        user_id: user_id.to_string(),
                        name: name.clone(),
                        scopes: scopes.clone(),
                        allowed_ips: vec![],
                        rate_limit_override_rpm: None,
                        rate_limit_override_tpm: None,
                        max_concurrent_requests: None,
                        expires_at: None,
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp)
            })
            .await
    }

    pub async fn rotate_key(
        &self,
        user_id: Uuid,
        key_id: Uuid,
        scopes: Option<Vec<String>>,
    ) -> Result<proto::key_validation_v1::RotateKeyResponse, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = KeyManagementServiceClient::new(self.config.channel().await?);
                let response = client
                    .rotate_key(self.config.auth(Request::new(RotateKeyRequest {
                        key_id: key_id.to_string(),
                        user_id: user_id.to_string(),
                        revoked_by: user_id.to_string(),
                        reason: "rotated by user".into(),
                        scopes: scopes.clone().unwrap_or_default(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(response)
            })
            .await
    }

    pub async fn update_key(
        &self,
        user_id: Uuid,
        key_id: Uuid,
        name: String,
        scopes: Vec<String>,
    ) -> Result<proto::key_validation_v1::KeyMetadata, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = KeyManagementServiceClient::new(self.config.channel().await?);
                let response = client
                    .update_key_metadata(self.config.auth(Request::new(
                        UpdateKeyMetadataRequest {
                            key_id: key_id.to_string(),
                            user_id: user_id.to_string(),
                            name: name.clone(),
                            allowed_ips: vec![],
                            rate_limit_override_rpm: None,
                            rate_limit_override_tpm: None,
                            max_concurrent_requests: None,
                            scopes: scopes.clone(),
                        },
                    ))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                response
                    .key
                    .ok_or_else(|| BffError::Internal("key service returned no key".into()))
            })
            .await
    }

    pub async fn revoke_key(&self, user_id: Uuid, key_id: Uuid) -> Result<(), BffError> {
        self.config
            .retry_call(|| async {
                let mut client = KeyManagementServiceClient::new(self.config.channel().await?);
                client
                    .revoke_key(self.config.auth(Request::new(RevokeKeyRequest {
                        key_id: key_id.to_string(),
                        user_id: user_id.to_string(),
                        revoked_by: "self".into(),
                        reason: String::new(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?;
                Ok(())
            })
            .await
    }
}

#[derive(Clone)]
pub struct PlanGrpcClient {
    pub config: GrpcClientConfig,
}

impl PlanGrpcClient {
    pub fn new(config: GrpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn list_plans(&self) -> Result<Vec<proto::plan_resolution_v1::Plan>, BffError> {
        self.config
            .retry_call(|| async {
                let mut client =
                    SubscriptionManagementServiceClient::new(self.config.channel().await?);
                let resp = client
                    .list_active_plans(self.config.auth(Request::new(ListActivePlansRequest {}))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.plans)
            })
            .await
    }

    pub async fn get_plan(
        &self,
        plan_id: Uuid,
    ) -> Result<proto::plan_resolution_v1::Plan, BffError> {
        self.config
            .retry_call(|| async {
                let mut client =
                    SubscriptionManagementServiceClient::new(self.config.channel().await?);
                let resp = client
                    .get_plan(self.config.auth(Request::new(GetPlanRequest {
                        plan_id: plan_id.to_string(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.plan.ok_or(BffError::NotFound("plan not found".into()))
            })
            .await
    }

    pub async fn get_subscription(
        &self,
        user_id: Uuid,
    ) -> Result<proto::plan_resolution_v1::Subscription, BffError> {
        self.config
            .retry_call(|| async {
                let mut client =
                    SubscriptionManagementServiceClient::new(self.config.channel().await?);
                let resp = client
                    .get_active_subscription(self.config.auth(Request::new(
                        GetActiveSubscriptionRequest {
                            user_id: user_id.to_string(),
                        },
                    ))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.subscription
                    .ok_or(BffError::NotFound("no active subscription".into()))
            })
            .await
    }

    pub async fn create_subscription(
        &self,
        user_id: Uuid,
        plan_id: Uuid,
        terms_version: String,
        privacy_version: String,
    ) -> Result<proto::plan_resolution_v1::Subscription, BffError> {
        self.config
            .retry_call(|| async {
                let mut client =
                    SubscriptionManagementServiceClient::new(self.config.channel().await?);
                let resp = client
                    .create_subscription(self.config.auth(Request::new(
                        CreateSubscriptionRequest {
                            user_id: user_id.to_string(),
                            plan_id: plan_id.to_string(),
                            billing_cycle: "monthly".into(),
                            terms_version: terms_version.clone(),
                            privacy_version: privacy_version.clone(),
                        },
                    ))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.subscription
                    .ok_or(BffError::Internal("no subscription returned".into()))
            })
            .await
    }

    pub async fn accept_terms(
        &self,
        user_id: Uuid,
        terms_version: String,
        privacy_version: String,
        ip_address: String,
        user_agent: String,
        country: String,
    ) -> Result<String, BffError> {
        self.config
            .retry_call(|| async {
                let mut client =
                    SubscriptionManagementServiceClient::new(self.config.channel().await?);
                let response = client
                    .accept_terms(self.config.auth(Request::new(AcceptTermsRequest {
                        user_id: user_id.to_string(),
                        terms_version: terms_version.clone(),
                        privacy_version: privacy_version.clone(),
                        ip_address: ip_address.clone(),
                        user_agent: user_agent.clone(),
                        country: country.clone(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(response.accepted_at)
            })
            .await
    }

    pub async fn cancel_subscription(
        &self,
        user_id: Uuid,
        at_period_end: bool,
    ) -> Result<proto::plan_resolution_v1::Subscription, BffError> {
        self.config
            .retry_call(|| async {
                let mut client =
                    SubscriptionManagementServiceClient::new(self.config.channel().await?);
                let resp = client
                    .cancel_subscription(self.config.auth(Request::new(
                        CancelSubscriptionRequest {
                            user_id: user_id.to_string(),
                            at_period_end,
                        },
                    ))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.subscription
                    .ok_or(BffError::Internal("no subscription returned".into()))
            })
            .await
    }
}

#[derive(Clone)]
pub struct CatalogGrpcClient {
    pub config: GrpcClientConfig,
}

#[derive(Clone)]
pub struct WalletGrpcClient {
    pub config: GrpcClientConfig,
}

#[derive(Clone)]
pub struct InvoiceGrpcClient {
    pub config: GrpcClientConfig,
}

#[derive(Clone)]
pub struct PaymentGrpcClient {
    pub config: GrpcClientConfig,
}

#[derive(Clone)]
pub struct UsageGrpcClient {
    pub config: GrpcClientConfig,
}

impl CatalogGrpcClient {
    pub fn new(config: GrpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn list_models(
        &self,
        plan_id: Uuid,
        limit: Option<u32>,
        offset: Option<u32>,
        after_cursor: Option<String>,
    ) -> Result<Vec<proto::catalog_v1::Model>, BffError> {
        let limit = limit.unwrap_or(100).clamp(1, 500);
        let cursor = after_cursor;
        self.config
            .retry_call(|| async {
                let mut client = CatalogServiceClient::new(self.config.channel().await?);
                let resp = client
                    .list_models(self.config.auth(Request::new(ListModelsRequest {
                        active: Some(true),
                        plan_id: Some(plan_id.to_string()),
                        provider_id: None,
                        family: None,
                        limit: Some(limit),
                        offset,
                        after_cursor: cursor.clone(),
                    }))?)
                    .await
                    .map_err(map_grpc_status)?
                    .into_inner();
                Ok(resp.models)
            })
            .await
    }

    pub async fn get_model(
        &self,
        plan_id: Uuid,
        model_id: Uuid,
    ) -> Result<proto::catalog_v1::Model, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = CatalogServiceClient::new(self.config.channel().await?);
                let resp = client
                    .get_model(self.config.auth(Request::new(GetModelRequest {
                        model_id: model_id.to_string(),
                        plan_id: Some(plan_id.to_string()),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.model
                    .ok_or(BffError::NotFound("model not found".into()))
            })
            .await
    }
}

impl WalletGrpcClient {
    pub fn new(config: GrpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn get_wallet(
        &self,
        user_id: Uuid,
    ) -> Result<proto::billing_reservation_v1::Wallet, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = BillingWalletServiceClient::new(self.config.channel().await?);
                let resp = client
                    .get_wallet(self.config.auth(Request::new(GetWalletRequest {
                        user_id: user_id.to_string(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.wallet
                    .ok_or(BffError::NotFound("wallet not found".into()))
            })
            .await
    }
}

impl InvoiceGrpcClient {
    pub fn new(config: GrpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn list_invoices(
        &self,
        user_id: Uuid,
        limit: i32,
    ) -> Result<Vec<proto::invoice_v1::Invoice>, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = InvoiceServiceClient::new(self.config.channel().await?);
                let resp = client
                    .list_invoices_by_user(self.config.auth(Request::new(
                        ListInvoicesByUserRequest {
                            user_id: user_id.to_string(),
                            limit,
                        },
                    ))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.invoices)
            })
            .await
    }

    pub async fn get_invoice(
        &self,
        invoice_id: Uuid,
    ) -> Result<proto::invoice_v1::Invoice, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = InvoiceServiceClient::new(self.config.channel().await?);
                let resp = client
                    .get_invoice(self.config.auth(Request::new(GetInvoiceRequest {
                        invoice_id: invoice_id.to_string(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.invoice
                    .ok_or(BffError::NotFound("invoice not found".into()))
            })
            .await
    }
}

impl PaymentGrpcClient {
    pub fn new(config: GrpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn list_payment_methods(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<proto::payment_v1::PaymentMethod>, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = PaymentServiceClient::new(self.config.channel().await?);
                let resp = client
                    .list_payment_methods(self.config.auth(Request::new(
                        ListPaymentMethodsRequest {
                            user_id: user_id.to_string(),
                        },
                    ))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.payment_methods)
            })
            .await
    }

    pub async fn set_default_payment_method(
        &self,
        user_id: Uuid,
        payment_method_id: Uuid,
    ) -> Result<(), BffError> {
        self.config
            .retry_call(|| async {
                let mut client = PaymentServiceClient::new(self.config.channel().await?);
                client
                    .set_default_payment_method(self.config.auth(Request::new(
                        SetDefaultPaymentMethodRequest {
                            user_id: user_id.to_string(),
                            payment_method_id: payment_method_id.to_string(),
                        },
                    ))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?;
                Ok(())
            })
            .await
    }

    pub async fn create_top_up(
        &self,
        user_id: Uuid,
        amount: String,
        currency: String,
        idempotency_key: String,
        return_url: String,
        cancel_url: String,
    ) -> Result<proto::payment_v1::CreateTopUpResponse, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = PaymentServiceClient::new(self.config.channel().await?);
                let resp = client
                    .create_top_up(self.config.auth(Request::new(CreateTopUpRequest {
                        user_id: user_id.to_string(),
                        amount: amount.clone(),
                        currency: currency.clone(),
                        idempotency_key: idempotency_key.clone(),
                        return_url: return_url.clone(),
                        cancel_url: cancel_url.clone(),
                        kind: "top_up".into(),
                        reference_id: String::new(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp)
            })
            .await
    }

    pub async fn create_subscription_order(
        &self,
        user_id: Uuid,
        amount: String,
        currency: String,
        plan_id: Uuid,
        return_url: String,
        cancel_url: String,
    ) -> Result<proto::payment_v1::CreateTopUpResponse, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = PaymentServiceClient::new(self.config.channel().await?);
                let response = client
                    .create_top_up(self.config.auth(Request::new(CreateTopUpRequest {
                        user_id: user_id.to_string(),
                        amount: amount.clone(),
                        currency: currency.clone(),
                        idempotency_key: format!("subscription:{}:{}", user_id, plan_id),
                        return_url: return_url.clone(),
                        cancel_url: cancel_url.clone(),
                        kind: "subscription_charge".into(),
                        reference_id: plan_id.to_string(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(response)
            })
            .await
    }

    pub async fn create_paypal_vault_setup_token(
        &self,
        user_id: Uuid,
        return_url: String,
        cancel_url: String,
    ) -> Result<proto::payment_v1::CreatePayPalVaultSetupTokenResponse, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = PaymentServiceClient::new(self.config.channel().await?);
                client
                    .create_pay_pal_vault_setup_token(self.config.auth(Request::new(
                        CreatePayPalVaultSetupTokenRequest {
                            user_id: user_id.to_string(),
                            return_url: return_url.clone(),
                            cancel_url: cancel_url.clone(),
                        },
                    ))?)
                    .await
                    .map_err(map_grpc_status)
                    .map(|response| response.into_inner())
            })
            .await
    }

    pub async fn create_paypal_payment_token(
        &self,
        user_id: Uuid,
        setup_token_id: String,
        set_default: bool,
    ) -> Result<proto::payment_v1::CreatePayPalPaymentTokenResponse, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = PaymentServiceClient::new(self.config.channel().await?);
                client
                    .create_pay_pal_payment_token(self.config.auth(Request::new(
                        CreatePayPalPaymentTokenRequest {
                            user_id: user_id.to_string(),
                            setup_token_id: setup_token_id.clone(),
                            set_default,
                        },
                    ))?)
                    .await
                    .map_err(map_grpc_status)
                    .map(|response| response.into_inner())
            })
            .await
    }

    pub async fn capture_paypal_order(
        &self,
        user_id: Uuid,
        order_id: String,
    ) -> Result<proto::payment_v1::CapturePayPalOrderResponse, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = PaymentServiceClient::new(self.config.channel().await?);
                Ok(client
                    .capture_pay_pal_order(self.config.auth(Request::new(
                        proto::payment_v1::CapturePayPalOrderRequest {
                            user_id: user_id.to_string(),
                            order_id: order_id.clone(),
                        },
                    ))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner())
            })
            .await
    }
}

impl UsageGrpcClient {
    pub fn new(config: GrpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn current_month_usage(
        &self,
        user_id: Uuid,
    ) -> Result<proto::usage_v1::UsageSummary, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = UsageReportingServiceClient::new(self.config.channel().await?);
                let resp = client
                    .get_current_month_usage(self.config.auth(Request::new(
                        CurrentMonthUsageRequest {
                            user_id: user_id.to_string(),
                        },
                    ))?)
                    .await
                    .map_err(map_grpc_status)?
                    .into_inner();
                resp.summary
                    .ok_or(BffError::NotFound("usage not found".into()))
            })
            .await
    }

    pub async fn top_models(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<proto::usage_v1::TopModel>, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = UsageReportingServiceClient::new(self.config.channel().await?);
                let resp = client
                    .get_top_models_by_user(self.config.auth(Request::new(
                        TopModelsByUserRequest {
                            user_id: user_id.to_string(),
                            range: Some(default_usage_range()),
                            limit: 10,
                        },
                    ))?)
                    .await
                    .map_err(map_grpc_status)?
                    .into_inner();
                Ok(resp.models)
            })
            .await
    }

    pub async fn latency_stats(
        &self,
        user_id: Uuid,
    ) -> Result<proto::usage_v1::LatencyStatsResponse, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = UsageReportingServiceClient::new(self.config.channel().await?);
                let resp = client
                    .get_latency_stats_by_user(self.config.auth(Request::new(
                        LatencyStatsByUserRequest {
                            user_id: user_id.to_string(),
                            range: Some(default_usage_range()),
                        },
                    ))?)
                    .await
                    .map_err(map_grpc_status)?
                    .into_inner();
                Ok(resp)
            })
            .await
    }
}
