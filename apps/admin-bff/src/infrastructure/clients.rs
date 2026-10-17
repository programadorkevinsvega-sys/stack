use crate::domain::errors::BffError;
use common::{derive_kid, ServiceJwtClaims, ServiceTlsConfig};
use proto::account_selection_v1::account_selection_service_client::AccountSelectionServiceClient;
use proto::account_selection_v1::{
    GetAccountLoadRequest, ListAccountLoadsRequest, ResetCircuitBreakerRequest,
};
use proto::auth_v1::auth_service_client::AuthServiceClient;
use proto::auth_v1::{
    DeleteUserRequest, DisableUserRequest, GetUserRequest, ListUsersRequest, LoginRequest,
    RefreshRequest, UnlockAccountRequest, ValidateAccessTokenRequest,
};
use proto::billing_reservation_v1::billing_wallet_service_client::BillingWalletServiceClient;
use proto::billing_reservation_v1::{AdjustWalletRequest, GetWalletRequest};
use proto::catalog_v1::catalog_service_client::CatalogServiceClient;
use proto::catalog_v1::{
    DeprecateModelRequest, GetModelMetadataRequest, GetModelRequest, GetProviderPolicyRequest,
    ListModelsRequest, RegisterModelProviderRequest, RegisterModelRequest, SetModelPricingRequest,
    SetProviderPolicyRequest, UpdateModelCapabilitiesRequest, UpsertModelMetadataRequest,
};
use proto::credential_resolution_v1::credential_resolution_service_client::CredentialResolutionServiceClient;
use proto::credential_resolution_v1::{
    ListAccountsRequest, ListAccountsResponse, UpsertMantleConnectionRequest,
    UpsertMantleConnectionResponse,
};
use proto::file_v1::file_service_client::FileServiceClient;
use proto::file_v1::{
    GetStorageUsageRequest, GetVectorUsageRequest, ListFilesRequest, ListFoldersRequest,
};
use proto::invoice_v1::invoice_service_client::InvoiceServiceClient;
use proto::invoice_v1::{ChangeInvoiceStatusRequest, GetInvoiceRequest, ListInvoicesByUserRequest};
use proto::plan_resolution_v1::subscription_management_service_client::SubscriptionManagementServiceClient;
use proto::plan_resolution_v1::{
    CreatePlanRequest, DeactivatePlanRequest, GetPlanRequest, ListActivePlansRequest,
    UpdatePlanRequest,
};
use proto::usage_v1::usage_reporting_service_client::UsageReportingServiceClient;
use proto::usage_v1::{
    CreateNextMonthPartitionRequest, ErrorRateRequest, LatencyStatsRequest,
    RefreshMaterializedViewsRequest, TopModelsRequest, UsageByUserRequest,
};
use std::time::Duration;
use tonic::metadata::MetadataValue;
use tonic::transport::Channel;
use tonic::Request;
use uuid::Uuid;

#[derive(Clone)]
pub struct AdminJetStreamClient {
    context: async_nats::jetstream::Context,
}

impl AdminJetStreamClient {
    pub fn new(client: async_nats::Client) -> Self {
        Self {
            context: async_nats::jetstream::new(client),
        }
    }

    pub async fn dlq_depth(&self) -> Result<u64, BffError> {
        let mut stream = self
            .context
            .get_stream(events::jetstream::DLQ_STREAM)
            .await
            .map_err(|error| BffError::UpstreamUnavailable(error.to_string()))?;
        let info = stream
            .info()
            .await
            .map_err(|error| BffError::UpstreamUnavailable(error.to_string()))?;
        Ok(info.state.messages)
    }
}

const GRPC_MAX_RETRIES: u32 = 3;

#[derive(Clone)]
pub struct GrpcClientConfig {
    pub endpoint: String,
    pub jwt_secret: String,
    pub issuer: String,
    pub subject: String,
    pub timeout_secs: u64,
    pub tls_config: Option<ServiceTlsConfig>,
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
    pub role: String,
}

#[derive(Clone)]
pub struct AdminAuthGrpcClient {
    pub config: GrpcClientConfig,
}

impl AdminAuthGrpcClient {
    pub fn new(config: GrpcClientConfig) -> Self {
        Self { config }
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
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
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
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
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
                if user.role != "admin" {
                    return Err(BffError::Forbidden);
                }
                let user_id = Uuid::parse_str(&user.id)
                    .map_err(|e| BffError::Internal(format!("invalid user id: {e}")))?;
                Ok(ValidatedSession {
                    user_id,
                    email: user.email,
                    role: user.role,
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
                let user = resp
                    .user
                    .ok_or(BffError::Internal("no user from token".into()))?;
                if user.role != "admin" {
                    return Err(BffError::Forbidden);
                }
                Ok(user)
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

    pub async fn list_users(
        &self,
        limit: Option<u32>,
        after_cursor: Option<String>,
    ) -> Result<Vec<proto::auth_v1::User>, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                let resp = client
                    .list_users(self.config.auth(Request::new(ListUsersRequest {
                        limit: limit.unwrap_or(100),
                        offset: 0,
                        after_cursor: after_cursor.clone(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.users)
            })
            .await
    }

    pub async fn disable_user(
        &self,
        user_id: Uuid,
        admin_actor_id: Uuid,
    ) -> Result<bool, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                let resp = client
                    .disable_user(self.config.auth(Request::new(DisableUserRequest {
                        user_id: user_id.to_string(),
                        admin_actor_id: admin_actor_id.to_string(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.success)
            })
            .await
    }

    pub async fn delete_user(&self, user_id: Uuid, admin_actor_id: Uuid) -> Result<bool, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                let resp = client
                    .delete_user(self.config.auth(Request::new(DeleteUserRequest {
                        user_id: user_id.to_string(),
                        admin_actor_id: admin_actor_id.to_string(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.success)
            })
            .await
    }

    pub async fn unlock_account(
        &self,
        user_id: Uuid,
        admin_actor_id: Uuid,
    ) -> Result<bool, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AuthServiceClient::new(self.config.channel().await?);
                let resp = client
                    .unlock_account(self.config.auth(Request::new(UnlockAccountRequest {
                        user_id: user_id.to_string(),
                        admin_actor_id: admin_actor_id.to_string(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.success)
            })
            .await
    }
}

#[derive(Clone)]
pub struct AdminCatalogGrpcClient {
    pub config: GrpcClientConfig,
}

#[derive(Clone)]
pub struct AdminCredentialGrpcClient {
    pub config: GrpcClientConfig,
}

impl AdminCredentialGrpcClient {
    pub fn new(config: GrpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn upsert_mantle_connection(
        &self,
        account_id: Uuid,
        model_catalog_id: Uuid,
        api_key: String,
        base_url: String,
        expires_at: Option<String>,
        admin_actor_id: Uuid,
    ) -> Result<UpsertMantleConnectionResponse, BffError> {
        self.config
            .retry_call(|| async {
                let mut client =
                    CredentialResolutionServiceClient::new(self.config.channel().await?);
                client
                    .upsert_mantle_connection(self.config.auth(Request::new(
                        UpsertMantleConnectionRequest {
                            account_id: account_id.to_string(),
                            model_catalog_id: model_catalog_id.to_string(),
                            api_key: api_key.clone(),
                            base_url: base_url.clone(),
                            expires_at: expires_at.clone(),
                            admin_actor_id: admin_actor_id.to_string(),
                            client_ip: None,
                        },
                    ))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))
                    .map(|response| response.into_inner())
            })
            .await
    }

    pub async fn list_accounts(&self) -> Result<ListAccountsResponse, BffError> {
        self.config
            .retry_call(|| async {
                let mut client =
                    CredentialResolutionServiceClient::new(self.config.channel().await?);
                client
                    .list_accounts(self.config.auth(Request::new(ListAccountsRequest {}))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))
                    .map(|response| response.into_inner())
            })
            .await
    }
}

pub struct RegisterModelInput {
    pub provider_id: String,
    pub public_model_name: String,
    pub provider_model_id: String,
    pub family: Option<String>,
    pub context_window: i32,
    pub max_output_tokens: i32,
    pub supports_streaming: bool,
    pub supports_tools: bool,
    pub supports_vision: bool,
    pub admin_actor_id: Uuid,
}

pub struct ModelMetadataInput {
    pub serving_provider_id: Option<String>,
    pub description: Option<String>,
    pub architecture: Option<String>,
    pub parameter_count: Option<i64>,
    pub precision: Option<String>,
    pub quantization: Option<String>,
    pub metadata_json: String,
    pub source_url: Option<String>,
    pub source_name: Option<String>,
    pub source_updated_at: Option<String>,
    pub metadata_version: i32,
}

pub struct ProviderPolicyInput {
    pub provider_id: String,
    pub model_catalog_id: Option<String>,
    pub retention_mode: String,
    pub retention_days: Option<i32>,
    pub training_usage: String,
    pub provider_access: String,
    pub supported_retention_modes_json: String,
    pub data_residency_json: String,
    pub security_controls_json: String,
    pub compliance_json: String,
    pub limitations_json: String,
    pub privacy_policy_url: Option<String>,
    pub terms_url: Option<String>,
    pub documentation_url: Option<String>,
    pub source_url: Option<String>,
    pub policy_version: String,
}

impl AdminCatalogGrpcClient {
    pub fn new(config: GrpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn list_models(
        &self,
        limit: Option<u32>,
        offset: Option<u32>,
        after_cursor: Option<String>,
        active: Option<bool>,
    ) -> Result<Vec<proto::catalog_v1::Model>, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = CatalogServiceClient::new(self.config.channel().await?);
                let resp = client
                    .list_models(self.config.auth(Request::new(ListModelsRequest {
                        plan_id: None,
                        provider_id: None,
                        family: None,
                        active,
                        limit,
                        offset,
                        after_cursor: after_cursor.clone(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.models)
            })
            .await
    }

    pub async fn get_model(&self, model_id: Uuid) -> Result<proto::catalog_v1::Model, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = CatalogServiceClient::new(self.config.channel().await?);
                let resp = client
                    .get_model(self.config.auth(Request::new(GetModelRequest {
                        model_id: model_id.to_string(),
                        plan_id: None,
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.model
                    .ok_or(BffError::NotFound("model not found".into()))
            })
            .await
    }

    pub async fn register_model(
        &self,
        input: RegisterModelInput,
    ) -> Result<proto::catalog_v1::Model, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = CatalogServiceClient::new(self.config.channel().await?);
                let resp = client
                    .register_model(self.config.auth(Request::new(RegisterModelRequest {
                        provider_id: input.provider_id.clone(),
                        public_model_name: input.public_model_name.clone(),
                        provider_model_id: input.provider_model_id.clone(),
                        family: input.family.clone(),
                        context_window: input.context_window,
                        max_output_tokens: input.max_output_tokens,
                        supports_streaming: input.supports_streaming,
                        supports_tools: input.supports_tools,
                        supports_vision: input.supports_vision,
                        admin_actor_id: input.admin_actor_id.to_string(),
                        client_ip: None,
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.model
                    .ok_or(BffError::Internal("no model returned".into()))
            })
            .await
    }

    pub async fn deprecate_model(
        &self,
        model_id: Uuid,
        admin_actor_id: Uuid,
    ) -> Result<bool, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = CatalogServiceClient::new(self.config.channel().await?);
                let resp = client
                    .deprecate_model(self.config.auth(Request::new(DeprecateModelRequest {
                        model_id: model_id.to_string(),
                        admin_actor_id: admin_actor_id.to_string(),
                        client_ip: None,
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.success)
            })
            .await
    }

    pub async fn set_model_pricing(
        &self,
        model_catalog_id: String,
        aws_cost_per_1k_input: String,
        aws_cost_per_1k_output: String,
        price_per_1k_input: String,
        price_per_1k_output: String,
        admin_actor_id: Uuid,
    ) -> Result<proto::catalog_v1::ModelPricing, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = CatalogServiceClient::new(self.config.channel().await?);
                let resp = client
                    .set_model_pricing(self.config.auth(Request::new(SetModelPricingRequest {
                        model_catalog_id: model_catalog_id.clone(),
                        aws_cost_per_1k_input: aws_cost_per_1k_input.clone(),
                        aws_cost_per_1k_output: aws_cost_per_1k_output.clone(),
                        price_per_1k_input: price_per_1k_input.clone(),
                        price_per_1k_output: price_per_1k_output.clone(),
                        admin_actor_id: admin_actor_id.to_string(),
                        client_ip: None,
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.pricing
                    .ok_or(BffError::Internal("no pricing returned".into()))
            })
            .await
    }

    pub async fn update_capabilities(
        &self,
        model_id: Uuid,
        supports_streaming: Option<bool>,
        supports_tools: Option<bool>,
        supports_vision: Option<bool>,
        provider_model_id: Option<String>,
        admin_actor_id: Uuid,
    ) -> Result<proto::catalog_v1::Model, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = CatalogServiceClient::new(self.config.channel().await?);
                let resp = client
                    .update_model_capabilities(self.config.auth(Request::new(
                        UpdateModelCapabilitiesRequest {
                            model_id: model_id.to_string(),
                            supports_streaming,
                            supports_tools,
                            supports_vision,
                            provider_model_id: provider_model_id.clone(),
                            admin_actor_id: admin_actor_id.to_string(),
                            client_ip: None,
                        },
                    ))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.model
                    .ok_or(BffError::Internal("no model returned".into()))
            })
            .await
    }

    pub async fn upsert_metadata(
        &self,
        model_catalog_id: Uuid,
        input: ModelMetadataInput,
        admin_actor_id: Uuid,
    ) -> Result<proto::catalog_v1::ModelMetadata, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = CatalogServiceClient::new(self.config.channel().await?);
                let response = client
                    .upsert_model_metadata(self.config.auth(Request::new(
                        UpsertModelMetadataRequest {
                            model_catalog_id: model_catalog_id.to_string(),
                            serving_provider_id: input.serving_provider_id.clone(),
                            description: input.description.clone(),
                            architecture: input.architecture.clone(),
                            parameter_count: input.parameter_count,
                            precision: input.precision.clone(),
                            quantization: input.quantization.clone(),
                            metadata_json: input.metadata_json.clone(),
                            source_url: input.source_url.clone(),
                            source_name: input.source_name.clone(),
                            source_updated_at: input.source_updated_at.clone(),
                            metadata_version: input.metadata_version,
                            admin_actor_id: admin_actor_id.to_string(),
                            client_ip: None,
                        },
                    ))?)
                    .await
                    .map_err(|error| BffError::UpstreamUnavailable(error.to_string()))?
                    .into_inner();
                response
                    .metadata
                    .ok_or(BffError::Internal("no model metadata returned".into()))
            })
            .await
    }

    pub async fn get_metadata(
        &self,
        model_catalog_id: Uuid,
    ) -> Result<proto::catalog_v1::ModelMetadata, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = CatalogServiceClient::new(self.config.channel().await?);
                let response = client
                    .get_model_metadata(self.config.auth(Request::new(
                        GetModelMetadataRequest {
                            model_catalog_id: model_catalog_id.to_string(),
                        },
                    ))?)
                    .await
                    .map_err(|error| BffError::UpstreamUnavailable(error.to_string()))?
                    .into_inner();
                response
                    .metadata
                    .ok_or(BffError::NotFound("model metadata not found".into()))
            })
            .await
    }

    pub async fn set_provider_policy(
        &self,
        input: ProviderPolicyInput,
        admin_actor_id: Uuid,
    ) -> Result<proto::catalog_v1::ProviderPolicy, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = CatalogServiceClient::new(self.config.channel().await?);
                let response = client
                    .set_provider_policy(self.config.auth(Request::new(
                        SetProviderPolicyRequest {
                            provider_id: input.provider_id.clone(),
                            model_catalog_id: input.model_catalog_id.clone(),
                            retention_mode: input.retention_mode.clone(),
                            retention_days: input.retention_days,
                            training_usage: input.training_usage.clone(),
                            provider_access: input.provider_access.clone(),
                            supported_retention_modes_json:
                                input.supported_retention_modes_json.clone(),
                            data_residency_json: input.data_residency_json.clone(),
                            security_controls_json: input.security_controls_json.clone(),
                            compliance_json: input.compliance_json.clone(),
                            limitations_json: input.limitations_json.clone(),
                            privacy_policy_url: input.privacy_policy_url.clone(),
                            terms_url: input.terms_url.clone(),
                            documentation_url: input.documentation_url.clone(),
                            source_url: input.source_url.clone(),
                            policy_version: input.policy_version.clone(),
                            admin_actor_id: admin_actor_id.to_string(),
                            client_ip: None,
                        },
                    ))?)
                    .await
                    .map_err(|error| BffError::UpstreamUnavailable(error.to_string()))?
                    .into_inner();
                response
                    .policy
                    .ok_or(BffError::Internal("no provider policy returned".into()))
            })
            .await
    }

    pub async fn get_provider_policy(
        &self,
        provider_id: Uuid,
        model_catalog_id: Option<Uuid>,
    ) -> Result<proto::catalog_v1::ProviderPolicy, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = CatalogServiceClient::new(self.config.channel().await?);
                let response = client
                    .get_provider_policy(self.config.auth(Request::new(
                        GetProviderPolicyRequest {
                            provider_id: provider_id.to_string(),
                            model_catalog_id: model_catalog_id.map(|value| value.to_string()),
                        },
                    ))?)
                    .await
                    .map_err(|error| BffError::UpstreamUnavailable(error.to_string()))?
                    .into_inner();
                response
                    .policy
                    .ok_or(BffError::NotFound("provider policy not found".into()))
            })
            .await
    }

    pub async fn register_provider(
        &self,
        name: String,
        admin_actor_id: Uuid,
    ) -> Result<proto::catalog_v1::Provider, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = CatalogServiceClient::new(self.config.channel().await?);
                let resp = client
                    .register_model_provider(self.config.auth(Request::new(
                        RegisterModelProviderRequest {
                            name: name.clone(),
                            admin_actor_id: admin_actor_id.to_string(),
                            client_ip: None,
                        },
                    ))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.provider
                    .ok_or(BffError::Internal("no provider returned".into()))
            })
            .await
    }

    pub async fn list_providers(&self) -> Result<Vec<proto::catalog_v1::Provider>, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = CatalogServiceClient::new(self.config.channel().await?);
                let resp = client
                    .list_providers(
                        self.config
                            .auth(Request::new(proto::catalog_v1::ListProvidersRequest {}))?,
                    )
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.providers)
            })
            .await
    }
}

#[derive(Clone)]
pub struct AdminPlanGrpcClient {
    pub config: GrpcClientConfig,
}

pub struct CreatePlanInput {
    pub name: String,
    pub slug: String,
    pub requests_per_minute: i32,
    pub tokens_per_minute: i32,
    pub max_concurrent_requests: i32,
    pub monthly_token_quota: Option<i64>,
    pub price_multiplier: String,
    pub billing_mode: String,
    pub features_json: String,
    pub storage_bytes_quota: i64,
    pub max_file_size_bytes: i64,
    pub max_files: i64,
    pub embeddings_enabled: bool,
    pub rag_enabled: bool,
    pub qdrant_mode: String,
    pub price_amount: String,
    pub currency: String,
    pub trial_days: i32,
    pub max_api_keys: i64,
    pub overage_enabled: bool,
    pub fair_use_policy: String,
    pub grace_period_days: i32,
    pub sso_enabled: bool,
    pub sla_enabled: bool,
    pub mtls_enabled: bool,
    pub s3_api_enabled: bool,
    pub max_ingestion_files_per_hour: i64,
    pub max_ingestion_files_per_day: i64,
    pub max_ingestion_bytes_per_day: i64,
    pub max_ingestion_tokens_per_month: i64,
    pub max_concurrent_ingestions: i64,
    pub max_pending_ingestion_jobs: i64,
}

pub struct UpdatePlanInput {
    pub plan_id: Uuid,
    pub name: String,
    pub slug: String,
    pub requests_per_minute: i32,
    pub tokens_per_minute: i32,
    pub max_concurrent_requests: i32,
    pub monthly_token_quota: Option<i64>,
    pub price_multiplier: String,
    pub billing_mode: String,
    pub features_json: String,
    pub is_active: bool,
    pub storage_bytes_quota: i64,
    pub max_file_size_bytes: i64,
    pub max_files: i64,
    pub embeddings_enabled: bool,
    pub rag_enabled: bool,
    pub qdrant_mode: String,
    pub price_amount: String,
    pub currency: String,
    pub trial_days: i32,
    pub max_api_keys: i64,
    pub overage_enabled: bool,
    pub fair_use_policy: String,
    pub grace_period_days: i32,
    pub sso_enabled: bool,
    pub sla_enabled: bool,
    pub mtls_enabled: bool,
    pub s3_api_enabled: bool,
    pub max_ingestion_files_per_hour: i64,
    pub max_ingestion_files_per_day: i64,
    pub max_ingestion_bytes_per_day: i64,
    pub max_ingestion_tokens_per_month: i64,
    pub max_concurrent_ingestions: i64,
    pub max_pending_ingestion_jobs: i64,
}

impl AdminPlanGrpcClient {
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

    pub async fn create_plan(
        &self,
        input: CreatePlanInput,
    ) -> Result<proto::plan_resolution_v1::Plan, BffError> {
        self.config
            .retry_call(|| async {
                let mut client =
                    SubscriptionManagementServiceClient::new(self.config.channel().await?);
                let resp = client
                    .create_plan(self.config.auth(Request::new(CreatePlanRequest {
                        name: input.name.clone(),
                        slug: input.slug.clone(),
                        requests_per_minute: input.requests_per_minute,
                        tokens_per_minute: input.tokens_per_minute,
                        max_concurrent_requests: input.max_concurrent_requests,
                        monthly_token_quota: input.monthly_token_quota,
                        price_multiplier: input.price_multiplier.clone(),
                        billing_mode: input.billing_mode.clone(),
                        features_json: input.features_json.clone(),
                        storage_bytes_quota: input.storage_bytes_quota,
                        max_file_size_bytes: input.max_file_size_bytes,
                        max_files: input.max_files,
                        embeddings_enabled: input.embeddings_enabled,
                        rag_enabled: input.rag_enabled,
                        qdrant_mode: input.qdrant_mode.clone(),
                        price_amount: input.price_amount.clone(),
                        currency: input.currency.clone(),
                        trial_days: input.trial_days,
                        max_api_keys: input.max_api_keys,
                        overage_enabled: input.overage_enabled,
                        fair_use_policy: input.fair_use_policy.clone(),
                        grace_period_days: input.grace_period_days,
                        sso_enabled: input.sso_enabled,
                        sla_enabled: input.sla_enabled,
                        mtls_enabled: input.mtls_enabled,
                        s3_api_enabled: input.s3_api_enabled,
                        max_ingestion_files_per_hour: input.max_ingestion_files_per_hour,
                        max_ingestion_files_per_day: input.max_ingestion_files_per_day,
                        max_ingestion_bytes_per_day: input.max_ingestion_bytes_per_day,
                        max_ingestion_tokens_per_month: input.max_ingestion_tokens_per_month,
                        max_concurrent_ingestions: input.max_concurrent_ingestions,
                        max_pending_ingestion_jobs: input.max_pending_ingestion_jobs,
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.plan
                    .ok_or(BffError::Internal("no plan returned".into()))
            })
            .await
    }

    pub async fn update_plan(
        &self,
        input: UpdatePlanInput,
    ) -> Result<proto::plan_resolution_v1::Plan, BffError> {
        self.config
            .retry_call(|| async {
                let mut client =
                    SubscriptionManagementServiceClient::new(self.config.channel().await?);
                let resp = client
                    .update_plan(self.config.auth(Request::new(UpdatePlanRequest {
                        plan_id: input.plan_id.to_string(),
                        name: input.name.clone(),
                        slug: input.slug.clone(),
                        requests_per_minute: input.requests_per_minute,
                        tokens_per_minute: input.tokens_per_minute,
                        max_concurrent_requests: input.max_concurrent_requests,
                        monthly_token_quota: input.monthly_token_quota,
                        price_multiplier: input.price_multiplier.clone(),
                        billing_mode: input.billing_mode.clone(),
                        features_json: input.features_json.clone(),
                        is_active: input.is_active,
                        storage_bytes_quota: input.storage_bytes_quota,
                        max_file_size_bytes: input.max_file_size_bytes,
                        max_files: input.max_files,
                        embeddings_enabled: input.embeddings_enabled,
                        rag_enabled: input.rag_enabled,
                        qdrant_mode: input.qdrant_mode.clone(),
                        price_amount: input.price_amount.clone(),
                        currency: input.currency.clone(),
                        trial_days: input.trial_days,
                        max_api_keys: input.max_api_keys,
                        overage_enabled: input.overage_enabled,
                        fair_use_policy: input.fair_use_policy.clone(),
                        grace_period_days: input.grace_period_days,
                        sso_enabled: input.sso_enabled,
                        sla_enabled: input.sla_enabled,
                        mtls_enabled: input.mtls_enabled,
                        s3_api_enabled: input.s3_api_enabled,
                        max_ingestion_files_per_hour: input.max_ingestion_files_per_hour,
                        max_ingestion_files_per_day: input.max_ingestion_files_per_day,
                        max_ingestion_bytes_per_day: input.max_ingestion_bytes_per_day,
                        max_ingestion_tokens_per_month: input.max_ingestion_tokens_per_month,
                        max_concurrent_ingestions: input.max_concurrent_ingestions,
                        max_pending_ingestion_jobs: input.max_pending_ingestion_jobs,
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.plan
                    .ok_or(BffError::Internal("no plan returned".into()))
            })
            .await
    }

    pub async fn deactivate_plan(
        &self,
        plan_id: Uuid,
    ) -> Result<proto::plan_resolution_v1::Plan, BffError> {
        self.config
            .retry_call(|| async {
                let mut client =
                    SubscriptionManagementServiceClient::new(self.config.channel().await?);
                let resp = client
                    .deactivate_plan(self.config.auth(Request::new(DeactivatePlanRequest {
                        plan_id: plan_id.to_string(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.plan
                    .ok_or(BffError::Internal("no plan returned".into()))
            })
            .await
    }
}

#[derive(Clone)]
pub struct AdminWalletGrpcClient {
    pub config: GrpcClientConfig,
}

impl AdminWalletGrpcClient {
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

    pub async fn adjust_wallet(
        &self,
        user_id: Uuid,
        amount: String,
        currency: String,
        reason: String,
        admin_id: Uuid,
        idempotency_key: String,
    ) -> Result<proto::billing_reservation_v1::Wallet, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = BillingWalletServiceClient::new(self.config.channel().await?);
                let resp = client
                    .adjust_wallet(self.config.auth(Request::new(AdjustWalletRequest {
                        user_id: user_id.to_string(),
                        amount: amount.clone(),
                        currency: currency.clone(),
                        reason: reason.clone(),
                        admin_id: admin_id.to_string(),
                        idempotency_key: idempotency_key.clone(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.wallet
                    .ok_or(BffError::Internal("no wallet returned".into()))
            })
            .await
    }
}

#[derive(Clone)]
pub struct AdminInvoiceGrpcClient {
    pub config: GrpcClientConfig,
}

impl AdminInvoiceGrpcClient {
    pub fn new(config: GrpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn list_invoices_by_user(
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

    pub async fn open_invoice(
        &self,
        invoice_id: Uuid,
    ) -> Result<proto::invoice_v1::Invoice, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = InvoiceServiceClient::new(self.config.channel().await?);
                let resp = client
                    .open_invoice(self.config.auth(Request::new(ChangeInvoiceStatusRequest {
                        invoice_id: invoice_id.to_string(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.invoice
                    .ok_or(BffError::Internal("no invoice returned".into()))
            })
            .await
    }

    pub async fn void_invoice(
        &self,
        invoice_id: Uuid,
    ) -> Result<proto::invoice_v1::Invoice, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = InvoiceServiceClient::new(self.config.channel().await?);
                let resp = client
                    .void_invoice(self.config.auth(Request::new(ChangeInvoiceStatusRequest {
                        invoice_id: invoice_id.to_string(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.invoice
                    .ok_or(BffError::Internal("no invoice returned".into()))
            })
            .await
    }

    pub async fn mark_uncollectible(
        &self,
        invoice_id: Uuid,
    ) -> Result<proto::invoice_v1::Invoice, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = InvoiceServiceClient::new(self.config.channel().await?);
                let resp = client
                    .mark_invoice_uncollectible(self.config.auth(Request::new(
                        ChangeInvoiceStatusRequest {
                            invoice_id: invoice_id.to_string(),
                        },
                    ))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.invoice
                    .ok_or(BffError::Internal("no invoice returned".into()))
            })
            .await
    }
}

#[derive(Clone)]
pub struct AdminUsageGrpcClient {
    pub config: GrpcClientConfig,
}

#[derive(Clone)]
pub struct AdminFileGrpcClient {
    pub config: GrpcClientConfig,
}

impl AdminFileGrpcClient {
    pub fn new(config: GrpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn list_files(&self, user_id: Uuid) -> Result<Vec<proto::file_v1::File>, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                let response = client
                    .list_files(self.config.auth(Request::new(ListFilesRequest {
                        user_id: user_id.to_string(),
                        folder_id: None,
                    }))?)
                    .await
                    .map_err(|error| BffError::UpstreamUnavailable(error.to_string()))?
                    .into_inner();
                Ok(response.files)
            })
            .await
    }

    pub async fn list_folders(
        &self,
        user_id: Uuid,
    ) -> Result<Vec<proto::file_v1::Folder>, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = FileServiceClient::new(self.config.channel().await?);
                let response = client
                    .list_folders(self.config.auth(Request::new(ListFoldersRequest {
                        user_id: user_id.to_string(),
                        parent_id: None,
                    }))?)
                    .await
                    .map_err(|error| BffError::UpstreamUnavailable(error.to_string()))?
                    .into_inner();
                Ok(response.folders)
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
                    .map(|response| response.into_inner())
                    .map_err(|error| BffError::UpstreamUnavailable(error.to_string()))
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
                    .map(|response| response.into_inner())
                    .map_err(|error| BffError::UpstreamUnavailable(error.to_string()))
            })
            .await
    }
}

impl AdminUsageGrpcClient {
    pub fn new(config: GrpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn get_usage_by_user(
        &self,
        user_id: Uuid,
        start: Option<String>,
        end: Option<String>,
    ) -> Result<proto::usage_v1::UsageSummary, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = UsageReportingServiceClient::new(self.config.channel().await?);
                let resp = client
                    .get_usage_by_user(self.config.auth(Request::new(UsageByUserRequest {
                        user_id: user_id.to_string(),
                        range: Some(proto::usage_v1::TimeRange {
                            start: start.clone().unwrap_or_default(),
                            end: end.clone().unwrap_or_default(),
                        }),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.summary
                    .ok_or(BffError::NotFound("usage not found".into()))
            })
            .await
    }

    pub async fn top_models(&self) -> Result<Vec<proto::usage_v1::TopModel>, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = UsageReportingServiceClient::new(self.config.channel().await?);
                let resp = client
                    .get_top_models(self.config.auth(Request::new(TopModelsRequest {
                        range: None,
                        limit: 20,
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.models)
            })
            .await
    }

    pub async fn error_rate(
        &self,
        start: Option<String>,
        end: Option<String>,
    ) -> Result<proto::usage_v1::ErrorRateResponse, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = UsageReportingServiceClient::new(self.config.channel().await?);
                let resp = client
                    .get_error_rate(self.config.auth(Request::new(ErrorRateRequest {
                        range: Some(proto::usage_v1::TimeRange {
                            start: start.clone().unwrap_or_default(),
                            end: end.clone().unwrap_or_default(),
                        }),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp)
            })
            .await
    }

    pub async fn latency_stats(
        &self,
        start: Option<String>,
        end: Option<String>,
    ) -> Result<proto::usage_v1::LatencyStatsResponse, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = UsageReportingServiceClient::new(self.config.channel().await?);
                let resp = client
                    .get_latency_stats(self.config.auth(Request::new(LatencyStatsRequest {
                        range: Some(proto::usage_v1::TimeRange {
                            start: start.clone().unwrap_or_default(),
                            end: end.clone().unwrap_or_default(),
                        }),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp)
            })
            .await
    }

    pub async fn refresh_views(&self) -> Result<bool, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = UsageReportingServiceClient::new(self.config.channel().await?);
                let resp = client
                    .refresh_materialized_views(
                        self.config
                            .auth(Request::new(RefreshMaterializedViewsRequest {}))?,
                    )
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.refreshed)
            })
            .await
    }

    pub async fn create_next_month_partition(&self) -> Result<String, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = UsageReportingServiceClient::new(self.config.channel().await?);
                let resp = client
                    .create_next_month_partition(
                        self.config
                            .auth(Request::new(CreateNextMonthPartitionRequest {}))?,
                    )
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.partition_name)
            })
            .await
    }
}

#[derive(Clone)]
pub struct AdminRouterGrpcClient {
    pub config: GrpcClientConfig,
}

impl AdminRouterGrpcClient {
    pub fn new(config: GrpcClientConfig) -> Self {
        Self { config }
    }

    pub async fn reset_circuit_breaker(&self, account_id: Uuid) -> Result<bool, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AccountSelectionServiceClient::new(self.config.channel().await?);
                let resp = client
                    .reset_circuit_breaker(self.config.auth(Request::new(
                        ResetCircuitBreakerRequest {
                            account_id: account_id.to_string(),
                        },
                    ))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.closed)
            })
            .await
    }

    pub async fn get_account_load(
        &self,
        account_id: Uuid,
    ) -> Result<proto::account_selection_v1::AccountLoad, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AccountSelectionServiceClient::new(self.config.channel().await?);
                let resp = client
                    .get_account_load(self.config.auth(Request::new(GetAccountLoadRequest {
                        account_id: account_id.to_string(),
                    }))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                resp.load
                    .ok_or(BffError::NotFound("account load not found".into()))
            })
            .await
    }

    pub async fn list_account_loads(
        &self,
        model_catalog_id: String,
    ) -> Result<Vec<proto::account_selection_v1::AccountLoad>, BffError> {
        self.config
            .retry_call(|| async {
                let mut client = AccountSelectionServiceClient::new(self.config.channel().await?);
                let resp = client
                    .list_account_loads(self.config.auth(Request::new(
                        ListAccountLoadsRequest {
                            model_catalog_id: model_catalog_id.clone(),
                        },
                    ))?)
                    .await
                    .map_err(|e| BffError::UpstreamUnavailable(e.to_string()))?
                    .into_inner();
                Ok(resp.loads)
            })
            .await
    }
}
