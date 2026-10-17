use crate::domain::errors::GatewayError;
use common::{derive_kid, ServiceJwtClaims, ServiceTlsConfig};
use proto::account_selection_v1::account_selection_service_client::AccountSelectionServiceClient;
use proto::account_selection_v1::{ReportOutcomeRequest, SelectAccountRequest};
use proto::billing_reservation_v1::billing_reservation_service_client::BillingReservationServiceClient;
use proto::billing_reservation_v1::{
    ConfirmDebitRequest, ReleaseReservationRequest, ReserveRequest,
};
use proto::catalog_v1::catalog_service_client::CatalogServiceClient;
use proto::catalog_v1::{GetCurrentPricingRequest, GetModelByPublicNameRequest};
use proto::credential_resolution_v1::credential_resolution_service_client::CredentialResolutionServiceClient;
use proto::credential_resolution_v1::{GetCredentialsRequest, GetMantleConnectionRequest};
use proto::key_validation_v1::key_validation_service_client::KeyValidationServiceClient;
use proto::key_validation_v1::ValidateKeyRequest;
use proto::plan_resolution_v1::plan_resolution_service_client::PlanResolutionServiceClient;
use proto::plan_resolution_v1::GetPlanLimitsRequest;
use rust_decimal::Decimal;
use tonic::metadata::MetadataValue;
use tonic::transport::Channel;
use tonic::Request;
use uuid::Uuid;

#[derive(Clone)]
pub struct ServiceClientConfig {
    pub endpoint: String,
    pub jwt_secret: String,
    pub issuer: String,
    pub subject: String,
    pub tls_config: Option<ServiceTlsConfig>,
}

impl ServiceClientConfig {
    fn validate(&self) -> Result<(), GatewayError> {
        if self.endpoint.trim().is_empty()
            || self.jwt_secret.len() < 32
            || self.issuer.trim().is_empty()
            || self.subject.trim().is_empty()
        {
            return Err(GatewayError::Internal(
                "invalid service client configuration".into(),
            ));
        }
        Ok(())
    }

    fn token(&self) -> Result<String, GatewayError> {
        let claims = ServiceJwtClaims::new(&self.issuer, &self.subject, 60)
            .map_err(|e| GatewayError::Internal(e.to_string()))?;
        let kid = derive_kid(self.jwt_secret.as_bytes());
        claims
            .sign_with_kid(&kid, self.jwt_secret.as_bytes())
            .map_err(|e| GatewayError::Internal(e.to_string()))
    }

    async fn channel(&self) -> Result<Channel, GatewayError> {
        self.validate()?;
        let endpoint = tonic::transport::Endpoint::from_shared(self.endpoint.clone())
            .map_err(|e| GatewayError::Internal(e.to_string()))?;
        if let Some(ref tls_config) = self.tls_config {
            let endpoint =
                common::tls::configure_client_endpoint(endpoint, &self.endpoint, tls_config)
                    .await
                    .map_err(|e| GatewayError::Internal(e.to_string()))?;
            endpoint
                .connect()
                .await
                .map_err(|e| GatewayError::UpstreamUnavailable(e.to_string()))
        } else {
            endpoint
                .connect()
                .await
                .map_err(|e| GatewayError::UpstreamUnavailable(e.to_string()))
        }
    }

    fn auth<T>(&self, mut request: Request<T>) -> Result<Request<T>, GatewayError> {
        let auth = format!("Bearer {}", self.token()?);
        request.metadata_mut().insert(
            "authorization",
            MetadataValue::try_from(auth).map_err(|e| GatewayError::Internal(e.to_string()))?,
        );
        common::observability::inject_tonic_context(&mut request);
        Ok(request)
    }
}

#[derive(Clone)]
pub struct KeyClient {
    pub config: ServiceClientConfig,
}

#[derive(Clone)]
pub struct ValidatedKey {
    pub api_key_id: Uuid,
    pub user_id: Uuid,
    pub status: String,
    pub scopes: Vec<String>,
    pub rate_limit_override_rpm: Option<i32>,
    pub rate_limit_override_tpm: Option<i32>,
    pub max_concurrent_requests: Option<i32>,
}

impl ValidatedKey {
    pub fn has_scope(&self, required: &str) -> bool {
        self.scopes.iter().any(|scope| scope == required)
    }
}

impl KeyClient {
    pub async fn validate(
        &self,
        api_key: &str,
        source_ip: Option<String>,
    ) -> Result<ValidatedKey, GatewayError> {
        let mut client = KeyValidationServiceClient::new(self.config.channel().await?);
        let response = client
            .validate_key(self.config.auth(Request::new(ValidateKeyRequest {
                api_key: api_key.into(),
                source_ip,
                required_scope: Some("inference:write".into()),
            }))?)
            .await
            .map_err(|e| GatewayError::UpstreamUnavailable(e.to_string()))?
            .into_inner();
        if !response.is_valid {
            return Err(GatewayError::InvalidApiKey);
        }
        if response.status != "active" {
            return Err(GatewayError::ApiKeyInactive);
        }
        Ok(ValidatedKey {
            api_key_id: parse_uuid(&response.api_key_id, "api_key_id")?,
            user_id: parse_uuid(&response.user_id, "user_id")?,
            status: response.status,
            scopes: response.scopes,
            rate_limit_override_rpm: response.rate_limit_override_rpm,
            rate_limit_override_tpm: response.rate_limit_override_tpm,
            max_concurrent_requests: response.max_concurrent_requests,
        })
    }
}

#[derive(Clone)]
pub struct PlanClient {
    pub config: ServiceClientConfig,
}

#[derive(Clone)]
pub struct PlanLimits {
    pub has_active_subscription: bool,
    pub requests_per_minute: i32,
    pub tokens_per_minute: i32,
    pub max_concurrent_requests: i32,
    pub plan_id: String,
}

impl PlanClient {
    pub async fn get_plan_limits(&self, user_id: Uuid) -> Result<PlanLimits, GatewayError> {
        let mut client = PlanResolutionServiceClient::new(self.config.channel().await?);
        let response = client
            .get_plan_limits(self.config.auth(Request::new(GetPlanLimitsRequest {
                user_id: user_id.to_string(),
            }))?)
            .await
            .map_err(|e| GatewayError::UpstreamUnavailable(e.to_string()))?
            .into_inner();
        Ok(PlanLimits {
            has_active_subscription: response.has_active_subscription,
            requests_per_minute: response.requests_per_minute,
            tokens_per_minute: response.tokens_per_minute,
            max_concurrent_requests: response.max_concurrent_requests,
            plan_id: response.plan_id,
        })
    }
}

#[derive(Clone)]
pub struct CatalogClient {
    pub config: ServiceClientConfig,
}

#[derive(Clone)]
pub struct CatalogModel {
    pub model_catalog_id: Uuid,
    pub provider_model_id: String,
    pub price_per_1k_input: Decimal,
    pub price_per_1k_output: Decimal,
}

impl CatalogClient {
    pub async fn get_model_by_public_name(
        &self,
        public_model_name: &str,
        plan_id: Option<&str>,
    ) -> Result<CatalogModel, GatewayError> {
        let mut client = CatalogServiceClient::new(self.config.channel().await?);
        let model = client
            .get_model_by_public_name(self.config.auth(Request::new(
                GetModelByPublicNameRequest {
                    public_model_name: public_model_name.into(),
                    plan_id: plan_id.map(ToOwned::to_owned),
                },
            ))?)
            .await
            .map_err(|_| GatewayError::ModelNotAvailable)?
            .into_inner()
            .model
            .ok_or(GatewayError::ModelNotAvailable)?;
        let pricing = client
            .get_current_pricing(self.config.auth(Request::new(GetCurrentPricingRequest {
                model_catalog_id: model.id.clone(),
                plan_id: plan_id.map(ToOwned::to_owned),
            }))?)
            .await
            .map_err(|e| GatewayError::UpstreamUnavailable(e.to_string()))?
            .into_inner()
            .pricing
            .ok_or(GatewayError::ModelNotAvailable)?;
        Ok(CatalogModel {
            model_catalog_id: parse_uuid(&model.id, "model_catalog_id")?,
            provider_model_id: model.provider_model_id,
            price_per_1k_input: pricing
                .price_per_1k_input
                .parse()
                .map_err(|e| GatewayError::Internal(format!("invalid input price: {e}")))?,
            price_per_1k_output: pricing
                .price_per_1k_output
                .parse()
                .map_err(|e| GatewayError::Internal(format!("invalid output price: {e}")))?,
        })
    }
}

#[derive(Clone)]
pub struct BillingClient {
    pub config: ServiceClientConfig,
}

#[derive(Clone)]
pub struct Reservation {
    pub approved: bool,
    pub reservation_id: Uuid,
}

impl BillingClient {
    pub async fn reserve(
        &self,
        user_id: Uuid,
        request_id: Uuid,
        estimated_tokens: i64,
        amount: Decimal,
    ) -> Result<Reservation, GatewayError> {
        let mut client = BillingReservationServiceClient::new(self.config.channel().await?);
        let response = client
            .reserve_balance(self.config.auth(Request::new(ReserveRequest {
                user_id: user_id.to_string(),
                idempotency_key: request_id.to_string(),
                estimated_amount: amount.to_string(),
                estimated_max_tokens: estimated_tokens.clamp(0, i64::from(i32::MAX)) as i32,
            }))?)
            .await
            .map_err(|e| GatewayError::UpstreamUnavailable(e.to_string()))?
            .into_inner();
        Ok(Reservation {
            approved: response.approved,
            reservation_id: if response.reservation_id.is_empty() {
                Uuid::nil()
            } else {
                parse_uuid(&response.reservation_id, "reservation_id")?
            },
        })
    }

    pub async fn release(&self, user_id: Uuid, reservation_id: Uuid) {
        if reservation_id.is_nil() {
            return;
        }
        if let Ok(mut client) = self
            .config
            .channel()
            .await
            .map(BillingReservationServiceClient::new)
        {
            let Ok(request) = self.config.auth(Request::new(ReleaseReservationRequest {
                reservation_id: reservation_id.to_string(),
                user_id: user_id.to_string(),
            })) else {
                return;
            };
            let _ = client.release_reservation(request).await;
        }
    }

    pub async fn confirm(
        &self,
        user_id: Uuid,
        reservation_id: Uuid,
        request_id: Uuid,
        model_catalog_id: Uuid,
        input_tokens: u32,
        output_tokens: u32,
    ) -> Result<(), GatewayError> {
        let mut client = BillingReservationServiceClient::new(self.config.channel().await?);
        client
            .confirm_debit(self.config.auth(Request::new(ConfirmDebitRequest {
                reservation_id: reservation_id.to_string(),
                user_id: user_id.to_string(),
                request_id: request_id.to_string(),
                actual_input_tokens: input_tokens.to_string(),
                actual_output_tokens: output_tokens.to_string(),
                model_catalog_id: model_catalog_id.to_string(),
            }))?)
            .await
            .map_err(|e| GatewayError::UpstreamUnavailable(e.to_string()))?;
        Ok(())
    }
}

#[derive(Clone)]
pub struct ModelRouterClient {
    pub config: ServiceClientConfig,
}

#[derive(Clone)]
pub struct SelectedAccount {
    pub account_id: Uuid,
    pub provider_model_id: String,
}

impl ModelRouterClient {
    pub async fn select_account(
        &self,
        model_catalog_id: Uuid,
        request_id: Uuid,
        estimated_tokens: i64,
    ) -> Result<SelectedAccount, GatewayError> {
        let mut client = AccountSelectionServiceClient::new(self.config.channel().await?);
        let response = client
            .select_account(self.config.auth(Request::new(SelectAccountRequest {
                model_catalog_id: model_catalog_id.to_string(),
                request_id: request_id.to_string(),
                estimated_tokens,
            }))?)
            .await
            .map_err(|e| GatewayError::UpstreamUnavailable(e.to_string()))?
            .into_inner();
        if !response.found {
            return Err(GatewayError::ModelNotAvailable);
        }
        Ok(SelectedAccount {
            account_id: parse_uuid(&response.account_id, "account_id")?,
            provider_model_id: response.provider_model_id,
        })
    }

    pub async fn report_outcome(
        &self,
        account_id: Uuid,
        request_id: Uuid,
        success: bool,
        error_code: Option<String>,
        input_tokens: i64,
        output_tokens: i64,
    ) {
        if let Ok(mut client) = self
            .config
            .channel()
            .await
            .map(AccountSelectionServiceClient::new)
        {
            let request = Request::new(ReportOutcomeRequest {
                account_id: account_id.to_string(),
                request_id: request_id.to_string(),
                success,
                error_code,
                latency_ms: 0,
                input_tokens,
                output_tokens,
            });
            let Ok(request) = self.config.auth(request) else {
                return;
            };
            let _ = client.report_outcome(request).await;
        }
    }
}

#[derive(Clone)]
pub struct CredentialClient {
    pub config: ServiceClientConfig,
}

#[derive(Clone)]
pub struct AwsCredentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: String,
    pub expires_at: String,
    pub region: String,
}

impl CredentialClient {
    pub async fn get_credentials(&self, account_id: Uuid) -> Result<AwsCredentials, GatewayError> {
        let mut client = CredentialResolutionServiceClient::new(self.config.channel().await?);
        let response = client
            .get_credentials(self.config.auth(Request::new(GetCredentialsRequest {
                account_id: account_id.to_string(),
            }))?)
            .await
            .map_err(|e| GatewayError::UpstreamUnavailable(e.to_string()))?
            .into_inner();
        Ok(AwsCredentials {
            access_key_id: response.access_key_id,
            secret_access_key: response.secret_access_key,
            session_token: response.session_token,
            expires_at: response.expires_at,
            region: response.region,
        })
    }

    pub async fn get_mantle_connection(
        &self,
        account_id: Uuid,
        model_catalog_id: Uuid,
    ) -> Result<Option<MantleConnection>, GatewayError> {
        let mut client = CredentialResolutionServiceClient::new(self.config.channel().await?);
        let response = client
            .get_mantle_connection(self.config.auth(Request::new(GetMantleConnectionRequest {
                account_id: account_id.to_string(),
                model_catalog_id: model_catalog_id.to_string(),
            }))?)
            .await
            .map_err(|e| GatewayError::UpstreamUnavailable(e.to_string()))?
            .into_inner();
        if !response.configured {
            return Ok(None);
        }
        let api_key = response
            .api_key
            .ok_or_else(|| GatewayError::Internal("Mantle connection has no API key".into()))?;
        if response.base_url.trim().is_empty() {
            return Err(GatewayError::Internal(
                "Mantle connection has no base URL".into(),
            ));
        }
        Ok(Some(MantleConnection {
            api_key,
            base_url: response.base_url,
        }))
    }
}

#[derive(Clone)]
pub struct MantleConnection {
    pub api_key: String,
    pub base_url: String,
}

fn parse_uuid(raw: &str, field: &str) -> Result<Uuid, GatewayError> {
    Uuid::parse_str(raw).map_err(|e| GatewayError::Internal(format!("invalid {field}: {e}")))
}
