use crate::domain::errors::BillingError;
use async_trait::async_trait;
use common::{derive_kid, ServiceJwtClaims, ServiceTlsConfig};
use proto::catalog_v1::catalog_service_client::CatalogServiceClient;
use proto::catalog_v1::GetCurrentPricingRequest;
use rust_decimal::Decimal;
use tonic::metadata::MetadataValue;
use tonic::transport::Channel;
use tonic::Request;
use uuid::Uuid;

#[derive(Clone)]
pub struct ModelPricing {
    pub aws_cost_per_1k_input: Decimal,
    pub aws_cost_per_1k_output: Decimal,
    pub price_per_1k_input: Decimal,
    pub price_per_1k_output: Decimal,
}

#[async_trait]
pub trait PricingResolver: Send + Sync {
    async fn get_current_pricing(
        &self,
        model_catalog_id: Uuid,
    ) -> Result<ModelPricing, BillingError>;
}

#[derive(Clone)]
pub struct CatalogPricingClient {
    channel: Channel,
    jwt_secret: String,
    issuer: String,
    subject: String,
}

impl CatalogPricingClient {
    pub async fn new(
        endpoint: String,
        jwt_secret: String,
        issuer: String,
        subject: String,
        tls_config: Option<ServiceTlsConfig>,
    ) -> Result<Self, BillingError> {
        if endpoint.trim().is_empty()
            || jwt_secret.len() < 32
            || issuer.trim().is_empty()
            || subject.trim().is_empty()
        {
            return Err(BillingError::Validation(
                "invalid catalog pricing client configuration".into(),
            ));
        }
        let channel_endpoint = Channel::from_shared(endpoint.clone())
            .map_err(|e| BillingError::Validation(e.to_string()))?;
        let channel_endpoint = if let Some(tls_config) = tls_config.as_ref() {
            common::tls::configure_client_endpoint(channel_endpoint, &endpoint, tls_config)
                .await
                .map_err(|e| BillingError::Validation(e.to_string()))?
        } else {
            channel_endpoint
        };
        let channel = channel_endpoint
            .connect()
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?;
        Ok(Self {
            channel,
            jwt_secret,
            issuer,
            subject,
        })
    }

    fn service_token(&self) -> Result<String, BillingError> {
        let claims = ServiceJwtClaims::new(&self.issuer, &self.subject, 60)
            .map_err(|e| BillingError::Validation(e.to_string()))?;
        let kid = derive_kid(self.jwt_secret.as_bytes());
        claims
            .sign_with_kid(&kid, self.jwt_secret.as_bytes())
            .map_err(|e| BillingError::Validation(e.to_string()))
    }
}

#[async_trait]
impl PricingResolver for CatalogPricingClient {
    async fn get_current_pricing(
        &self,
        model_catalog_id: Uuid,
    ) -> Result<ModelPricing, BillingError> {
        let mut client = CatalogServiceClient::new(self.channel.clone());
        let mut request = Request::new(GetCurrentPricingRequest {
            model_catalog_id: model_catalog_id.to_string(),
            plan_id: None,
        });
        let auth = format!("Bearer {}", self.service_token()?);
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(auth).map_err(|e| BillingError::Validation(e.to_string()))?,
        );
        common::observability::inject_tonic_context(&mut request);
        let pricing = client
            .get_current_pricing(request)
            .await
            .map_err(|e| BillingError::DependencyUnavailable(e.to_string()))?
            .into_inner()
            .pricing
            .ok_or_else(|| BillingError::DependencyUnavailable("catalog pricing missing".into()))?;
        Ok(ModelPricing {
            aws_cost_per_1k_input: pricing.aws_cost_per_1k_input.parse().map_err(|e| {
                BillingError::DependencyUnavailable(format!("invalid catalog aws input price: {e}"))
            })?,
            aws_cost_per_1k_output: pricing.aws_cost_per_1k_output.parse().map_err(|e| {
                BillingError::DependencyUnavailable(format!(
                    "invalid catalog aws output price: {e}"
                ))
            })?,
            price_per_1k_input: pricing.price_per_1k_input.parse().map_err(|e| {
                BillingError::DependencyUnavailable(format!("invalid catalog input price: {e}"))
            })?,
            price_per_1k_output: pricing.price_per_1k_output.parse().map_err(|e| {
                BillingError::DependencyUnavailable(format!("invalid catalog output price: {e}"))
            })?,
        })
    }
}
