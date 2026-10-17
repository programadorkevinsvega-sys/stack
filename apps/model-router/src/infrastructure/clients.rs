use crate::domain::entities::AccountCandidate;
use crate::domain::errors::ModelRouterError;
use async_trait::async_trait;
use common::{derive_kid, ServiceJwtClaims, ServiceTlsConfig};
use proto::catalog_v1::catalog_service_client::CatalogServiceClient;
use proto::catalog_v1::GetModelRequest;
use proto::credential_resolution_v1::credential_resolution_service_client::CredentialResolutionServiceClient;
use proto::credential_resolution_v1::ListHealthyAccountsForModelRequest;
use tonic::metadata::MetadataValue;
use tonic::transport::Channel;
use tonic::Request;
use uuid::Uuid;

#[async_trait]
pub trait CredentialResolver: Send + Sync {
    async fn list_healthy_accounts_for_model(
        &self,
        model_catalog_id: Uuid,
    ) -> Result<Vec<AccountCandidate>, ModelRouterError>;
}

#[async_trait]
pub trait CatalogResolver: Send + Sync {
    async fn provider_model_id(&self, model_catalog_id: Uuid) -> Result<String, ModelRouterError>;
}

#[derive(Clone)]
pub struct ServiceGrpcClientConfig {
    pub endpoint: String,
    pub jwt_secret: String,
    pub issuer: String,
    pub subject: String,
    pub tls_config: Option<ServiceTlsConfig>,
}

impl ServiceGrpcClientConfig {
    pub fn validate(&self) -> Result<(), ModelRouterError> {
        if self.endpoint.trim().is_empty()
            || self.jwt_secret.len() < 32
            || self.issuer.trim().is_empty()
            || self.subject.trim().is_empty()
        {
            return Err(ModelRouterError::Validation(
                "invalid service gRPC client configuration".into(),
            ));
        }
        Ok(())
    }

    fn token(&self) -> Result<String, ModelRouterError> {
        let claims = ServiceJwtClaims::new(&self.issuer, &self.subject, 60)
            .map_err(|e| ModelRouterError::Validation(e.to_string()))?;
        let kid = derive_kid(self.jwt_secret.as_bytes());
        claims
            .sign_with_kid(&kid, self.jwt_secret.as_bytes())
            .map_err(|e| ModelRouterError::Validation(e.to_string()))
    }

    async fn channel(&self) -> Result<Channel, ModelRouterError> {
        let endpoint = tonic::transport::Endpoint::from_shared(self.endpoint.clone())
            .map_err(|e| ModelRouterError::Validation(e.to_string()))?;
        if let Some(ref tls_config) = self.tls_config {
            let endpoint =
                common::tls::configure_client_endpoint(endpoint, &self.endpoint, tls_config)
                    .await
                    .map_err(|e| ModelRouterError::DependencyUnavailable(e.to_string()))?;
            endpoint
                .connect()
                .await
                .map_err(|e| ModelRouterError::DependencyUnavailable(e.to_string()))
        } else {
            endpoint
                .connect()
                .await
                .map_err(|e| ModelRouterError::DependencyUnavailable(e.to_string()))
        }
    }

    fn auth<T>(&self, mut request: Request<T>) -> Result<Request<T>, ModelRouterError> {
        let auth = format!("Bearer {}", self.token()?);
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(auth)
                .map_err(|e| ModelRouterError::Validation(e.to_string()))?,
        );
        common::observability::inject_tonic_context(&mut request);
        Ok(request)
    }
}

#[derive(Clone)]
pub struct CredentialManagerClient {
    pub config: ServiceGrpcClientConfig,
}

#[async_trait]
impl CredentialResolver for CredentialManagerClient {
    async fn list_healthy_accounts_for_model(
        &self,
        model_catalog_id: Uuid,
    ) -> Result<Vec<AccountCandidate>, ModelRouterError> {
        self.config.validate()?;
        let mut client = CredentialResolutionServiceClient::new(self.config.channel().await?);
        let request = self
            .config
            .auth(Request::new(ListHealthyAccountsForModelRequest {
                model_catalog_id: model_catalog_id.to_string(),
            }))?;
        let response = client
            .list_healthy_accounts_for_model(request)
            .await
            .map_err(|e| ModelRouterError::DependencyUnavailable(e.to_string()))?
            .into_inner();
        response
            .accounts
            .into_iter()
            .map(|account| {
                Ok(AccountCandidate {
                    account_id: Uuid::parse_str(&account.account_id).map_err(|e| {
                        ModelRouterError::DependencyUnavailable(format!(
                            "invalid account_id from credential-manager: {e}"
                        ))
                    })?,
                    aws_account_id: account.aws_account_id,
                    region: account.region,
                    max_tpm: i64::from(account.max_tpm),
                    max_rpm: i64::from(account.max_rpm),
                    priority_weight: i64::from(account.priority_weight),
                })
            })
            .collect()
    }
}

#[derive(Clone)]
pub struct CatalogClient {
    pub config: ServiceGrpcClientConfig,
}

#[async_trait]
impl CatalogResolver for CatalogClient {
    async fn provider_model_id(&self, model_catalog_id: Uuid) -> Result<String, ModelRouterError> {
        self.config.validate()?;
        let mut client = CatalogServiceClient::new(self.config.channel().await?);
        let request = self.config.auth(Request::new(GetModelRequest {
            model_id: model_catalog_id.to_string(),
            plan_id: None,
        }))?;
        let model = client
            .get_model(request)
            .await
            .map_err(|e| ModelRouterError::DependencyUnavailable(e.to_string()))?
            .into_inner()
            .model
            .ok_or(ModelRouterError::ModelNotFound)?;
        Ok(model.provider_model_id)
    }
}
