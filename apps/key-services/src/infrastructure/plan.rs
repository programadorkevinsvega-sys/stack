use crate::application::ports::ApiKeyPlanLimits;
use crate::domain::errors::KeyServiceError;
use async_trait::async_trait;
use common::{derive_kid, ServiceJwtClaims, ServiceTlsConfig};
use proto::plan_resolution_v1::plan_resolution_service_client::PlanResolutionServiceClient;
use proto::plan_resolution_v1::GetPlanLimitsRequest;
use std::sync::Arc;
use tonic::metadata::MetadataValue;
use tonic::transport::{Channel, Endpoint};
use tonic::Request;
use uuid::Uuid;

pub struct SubscriptionPlanLimitsClient {
    pub endpoint: String,
    pub secret: String,
    pub issuer: String,
    pub subject: String,
    pub tls: ServiceTlsConfig,
}

impl SubscriptionPlanLimitsClient {
    async fn channel(&self) -> Result<Channel, KeyServiceError> {
        let endpoint = Endpoint::from_shared(self.endpoint.clone())
            .map_err(|e| KeyServiceError::Configuration(e.to_string()))?;
        let endpoint = if self.tls.is_enabled() {
            common::tls::configure_client_endpoint(endpoint, &self.endpoint, &self.tls)
                .await
                .map_err(|e| KeyServiceError::DependencyUnavailable(e.to_string()))?
        } else {
            endpoint
        };
        endpoint
            .connect()
            .await
            .map_err(|e| KeyServiceError::DependencyUnavailable(e.to_string()))
    }
}

#[async_trait]
impl ApiKeyPlanLimits for SubscriptionPlanLimitsClient {
    async fn max_api_keys(&self, user_id: Uuid) -> Result<i64, KeyServiceError> {
        let claims = ServiceJwtClaims::new(&self.issuer, &self.subject, 60)
            .map_err(|e| KeyServiceError::Configuration(e.to_string()))?;
        let token = claims
            .sign_with_kid(&derive_kid(self.secret.as_bytes()), self.secret.as_bytes())
            .map_err(|e| KeyServiceError::Configuration(e.to_string()))?;
        let mut request = Request::new(GetPlanLimitsRequest {
            user_id: user_id.to_string(),
        });
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(format!("Bearer {token}"))
                .map_err(|e| KeyServiceError::Configuration(e.to_string()))?,
        );
        let response = PlanResolutionServiceClient::new(self.channel().await?)
            .get_plan_limits(request)
            .await
            .map_err(|e| KeyServiceError::DependencyUnavailable(e.to_string()))?
            .into_inner();
        if !response.has_active_subscription {
            return Err(KeyServiceError::SubscriptionRequired);
        }
        Ok(response.max_api_keys)
    }
}

pub type SharedPlanLimitsClient = Arc<SubscriptionPlanLimitsClient>;
