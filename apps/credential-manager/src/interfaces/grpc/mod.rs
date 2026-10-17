use crate::application::commands::UpsertMantleConnection;
use crate::application::queries::{
    GetAccount, GetCredentials, GetMantleConnection, ListAccounts, ListHealthyAccountsForModel,
    ListModelsForAccount,
};
use crate::domain::errors::CredentialError;
use proto::credential_resolution_v1::credential_resolution_service_server::CredentialResolutionService;
use proto::credential_resolution_v1::{
    AccountSummary, GetCredentialsRequest, GetCredentialsResponse, GetMantleConnectionRequest,
    GetMantleConnectionResponse, HealthCheckRequest, HealthCheckResponse, HealthyAccount,
    ListAccountsRequest, ListAccountsResponse, ListHealthyAccountsForModelRequest,
    ListHealthyAccountsForModelResponse, ListModelsForAccountRequest, ListModelsForAccountResponse,
    UpsertMantleConnectionRequest, UpsertMantleConnectionResponse,
};
use tonic::{Request, Response, Status};
use uuid::Uuid;

pub struct CredentialGrpcService {
    pub get_credentials: GetCredentials,
    pub get_account: GetAccount,
    pub list_models_for_account: ListModelsForAccount,
    pub list_healthy_accounts_for_model: ListHealthyAccountsForModel,
    pub list_accounts: ListAccounts,
    pub get_mantle_connection: GetMantleConnection,
    pub upsert_mantle_connection: UpsertMantleConnection,
}

fn parse_uuid(field: &str, raw: &str) -> Result<Uuid, Status> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument(format!("{field} is required")));
    }
    Uuid::parse_str(trimmed).map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
}

fn map_error(err: CredentialError) -> Status {
    match err {
        CredentialError::AccountNotFound(msg) => Status::not_found(msg),
        CredentialError::AccountInactive(msg) | CredentialError::AccountUnhealthy(msg) => {
            Status::failed_precondition(msg)
        }
        CredentialError::AccountAlreadyExists { account_id } => {
            Status::already_exists(format!("account already exists: {account_id}"))
        }
        CredentialError::Unauthorized(_) => Status::unauthenticated("unauthorized"),
        CredentialError::Validation(msg) | CredentialError::InvalidAuthMethod(msg) => {
            Status::invalid_argument(msg)
        }
        CredentialError::Configuration(_) => Status::internal("service misconfiguration"),
        CredentialError::DependencyUnavailable(_)
        | CredentialError::KmsEncryptionFailed { .. }
        | CredentialError::KmsDecryptionFailed { .. }
        | CredentialError::StsAssumeRoleFailed { .. } => {
            Status::unavailable("credential dependency unavailable")
        }
        CredentialError::CredentialExpired { account_id } => {
            Status::failed_precondition(format!("credential expired for account {account_id}"))
        }
    }
}

#[tonic::async_trait]
impl CredentialResolutionService for CredentialGrpcService {
    async fn get_credentials(
        &self,
        request: Request<GetCredentialsRequest>,
    ) -> Result<Response<GetCredentialsResponse>, Status> {
        let req = request.into_inner();
        let account_id = parse_uuid("account_id", &req.account_id)?;

        let token = self
            .get_credentials
            .execute(account_id)
            .await
            .map_err(map_error)?;

        let expires_at = token
            .expires_at
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

        Ok(Response::new(GetCredentialsResponse {
            access_key_id: token.access_key_id,
            secret_access_key: token.secret_access_key,
            session_token: token.session_token.unwrap_or_default(),
            expires_at,
            region: token.region,
        }))
    }

    async fn health_check(
        &self,
        request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        let req = request.into_inner();
        let account_id = parse_uuid("account_id", &req.account_id)?;

        let account = self
            .get_account
            .execute(account_id)
            .await
            .map_err(map_error)?
            .ok_or_else(|| Status::not_found("account not found"))?;

        Ok(Response::new(HealthCheckResponse {
            is_healthy: account.is_healthy,
            consecutive_errors: account.consecutive_errors as i32,
            last_error: account.last_error_code.unwrap_or_default(),
        }))
    }

    async fn list_models_for_account(
        &self,
        request: Request<ListModelsForAccountRequest>,
    ) -> Result<Response<ListModelsForAccountResponse>, Status> {
        let account_id = parse_uuid("account_id", &request.into_inner().account_id)?;
        let model_catalog_ids = self
            .list_models_for_account
            .execute(account_id)
            .await
            .map_err(map_error)?
            .into_iter()
            .map(|id| id.to_string())
            .collect();
        Ok(Response::new(ListModelsForAccountResponse {
            model_catalog_ids,
        }))
    }

    async fn list_healthy_accounts_for_model(
        &self,
        request: Request<ListHealthyAccountsForModelRequest>,
    ) -> Result<Response<ListHealthyAccountsForModelResponse>, Status> {
        let model_catalog_id =
            parse_uuid("model_catalog_id", &request.into_inner().model_catalog_id)?;
        let accounts = self
            .list_healthy_accounts_for_model
            .execute(model_catalog_id)
            .await
            .map_err(map_error)?
            .into_iter()
            .map(|account| HealthyAccount {
                account_id: account.id.to_string(),
                aws_account_id: account.aws_account_id,
                region: account.region,
                max_tpm: account.max_tpm,
                max_rpm: account.max_rpm,
                priority_weight: i32::from(account.priority_weight),
            })
            .collect();
        Ok(Response::new(ListHealthyAccountsForModelResponse {
            accounts,
        }))
    }

    async fn list_accounts(
        &self,
        request: Request<ListAccountsRequest>,
    ) -> Result<Response<ListAccountsResponse>, Status> {
        drop(request);
        let accounts = self.list_accounts.execute().await.map_err(map_error)?;
        Ok(Response::new(ListAccountsResponse {
            accounts: accounts
                .into_iter()
                .map(|account| AccountSummary {
                    account_id: account.id.to_string(),
                    account_alias: account.account_alias,
                    aws_account_id: account.aws_account_id,
                    region: account.region,
                    auth_method: account.auth_method,
                    is_healthy: account.is_healthy,
                    is_active: account.is_active,
                })
                .collect(),
        }))
    }

    async fn get_mantle_connection(
        &self,
        request: Request<GetMantleConnectionRequest>,
    ) -> Result<Response<GetMantleConnectionResponse>, Status> {
        let req = request.into_inner();
        let account_id = parse_uuid("account_id", &req.account_id)?;
        let model_catalog_id = parse_uuid("model_catalog_id", &req.model_catalog_id)?;
        let result = self
            .get_mantle_connection
            .execute(account_id, model_catalog_id)
            .await
            .map_err(map_error)?;
        let Some(secret) = result else {
            return Ok(Response::new(GetMantleConnectionResponse {
                configured: false,
                api_key: None,
                base_url: String::new(),
                expires_at: None,
            }));
        };
        Ok(Response::new(GetMantleConnectionResponse {
            configured: secret.api_key.is_some(),
            api_key: secret.api_key,
            base_url: secret.connection.base_url.unwrap_or_default(),
            expires_at: secret.connection.expires_at.map(|value| value.to_rfc3339()),
        }))
    }

    async fn upsert_mantle_connection(
        &self,
        request: Request<UpsertMantleConnectionRequest>,
    ) -> Result<Response<UpsertMantleConnectionResponse>, Status> {
        let claims = request
            .extensions()
            .get::<common::ServiceJwtClaims>()
            .ok_or_else(|| Status::unauthenticated("missing service identity"))?;
        if claims.sub != "admin-bff" {
            return Err(Status::permission_denied("admin identity required"));
        }
        let req = request.into_inner();
        let account_id = parse_uuid("account_id", &req.account_id)?;
        let model_catalog_id = parse_uuid("model_catalog_id", &req.model_catalog_id)?;
        let admin_actor_id = parse_uuid("admin_actor_id", &req.admin_actor_id)?;
        let expires_at = req
            .expires_at
            .as_deref()
            .map(|value| {
                chrono::DateTime::parse_from_rfc3339(value)
                    .map(|date| date.with_timezone(&chrono::Utc))
                    .map_err(|error| {
                        Status::invalid_argument(format!("invalid expires_at: {error}"))
                    })
            })
            .transpose()?;
        self.upsert_mantle_connection
            .execute(
                account_id,
                model_catalog_id,
                req.api_key,
                req.base_url,
                expires_at,
                admin_actor_id,
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(UpsertMantleConnectionResponse {
            account_id: account_id.to_string(),
            model_catalog_id: model_catalog_id.to_string(),
            base_url: "configured".to_string(),
            expires_at: expires_at.map(|value| value.to_rfc3339()),
            configured: true,
        }))
    }
}
