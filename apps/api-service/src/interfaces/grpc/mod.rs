use crate::application::commands::{
    DeprecateModel, RegisterModel, RegisterModelInput, RegisterModelProvider, SetModelPricing,
    SetProviderPolicy, UpdateModelCapabilities, UpsertModelMetadata,
};
use crate::application::ports::{AuditActorType, AuditContext};
use crate::application::queries::{
    GetCurrentPricing, GetModel, GetModelByPublicName, GetModelMetadata, GetPricingAt,
    GetProviderPolicy, ListModels, ListPricingHistory, ListProviders,
};
use crate::domain::entities::{
    ModelCapabilities, ModelCatalog, ModelMetadata, ModelPricing, ModelProvider, ProviderPolicy,
    ProviderPolicyInput, TokenPrice,
};
use crate::domain::errors::CatalogError;
use crate::domain::repositories::ModelQueryFilter;
use chrono::{DateTime, Utc};
use common::ServiceJwtClaims;
use proto::catalog_v1::catalog_service_server::CatalogService;
use proto::catalog_v1::{
    DeprecateModelRequest, DeprecateModelResponse, GetCurrentPricingRequest,
    GetCurrentPricingResponse, GetModelByPublicNameRequest, GetModelByPublicNameResponse,
    GetModelMetadataRequest, GetModelMetadataResponse, GetModelRequest, GetModelResponse,
    GetPricingAtRequest, GetPricingAtResponse, GetProviderPolicyRequest, GetProviderPolicyResponse,
    HealthCheckRequest, HealthCheckResponse, ListModelsRequest, ListModelsResponse,
    ListPricingHistoryRequest, ListPricingHistoryResponse, ListProvidersRequest,
    ListProvidersResponse, Model, ModelMetadata as ProtoModelMetadata,
    ModelPricing as ProtoModelPricing, Provider, ProviderPolicy as ProtoProviderPolicy,
    RegisterModelProviderRequest, RegisterModelProviderResponse, RegisterModelRequest,
    RegisterModelResponse, SetModelPricingRequest, SetModelPricingResponse,
    SetProviderPolicyRequest, SetProviderPolicyResponse, UpdateModelCapabilitiesRequest,
    UpdateModelCapabilitiesResponse, UpsertModelMetadataRequest, UpsertModelMetadataResponse,
};
use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::HashSet;
use std::net::IpAddr;
use std::str::FromStr;
use tonic::{Request, Response, Status};
use uuid::Uuid;

pub struct CatalogGrpcService {
    pub register_provider: RegisterModelProvider,
    pub register_model: RegisterModel,
    pub deprecate_model: DeprecateModel,
    pub set_pricing: SetModelPricing,
    pub update_capabilities: UpdateModelCapabilities,
    pub upsert_metadata: UpsertModelMetadata,
    pub set_provider_policy: SetProviderPolicy,
    pub list_models: ListModels,
    pub get_model: GetModel,
    pub get_by_name: GetModelByPublicName,
    pub get_metadata: GetModelMetadata,
    pub get_provider_policy: GetProviderPolicy,
    pub get_pricing: GetCurrentPricing,
    pub get_pricing_at: GetPricingAt,
    pub list_pricing_history: ListPricingHistory,
    pub list_providers: ListProviders,
    pub admin_callers: HashSet<String>,
}

fn map_error(error: CatalogError) -> Status {
    match error {
        CatalogError::ModelNotFound { .. } => Status::not_found("model not found"),
        CatalogError::ProviderNotFound { .. } => Status::not_found("provider not found"),
        CatalogError::ProviderInactive { .. } => Status::failed_precondition("provider inactive"),
        CatalogError::ModelNameAlreadyExists { name } => Status::already_exists(name),
        CatalogError::ProviderNameAlreadyExists { name } => Status::already_exists(name),
        CatalogError::NoActivePricing { .. } => Status::not_found("active pricing not found"),
        CatalogError::ModelDeprecated { .. } => Status::failed_precondition("model deprecated"),
        CatalogError::Unauthorized(_) => Status::unauthenticated("unauthorized"),
        CatalogError::Forbidden(_) => Status::permission_denied("forbidden"),
        CatalogError::DependencyUnavailable(_) => {
            Status::unavailable("catalog dependency unavailable")
        }
        CatalogError::Configuration(_) | CatalogError::Internal(_) => {
            Status::internal("catalog internal error")
        }
        CatalogError::Validation(message) => Status::invalid_argument(message),
    }
}

fn parse_uuid(field: &str, raw: &str) -> Result<Uuid, Status> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument(format!("{field} is required")));
    }
    Uuid::parse_str(trimmed).map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
}

fn parse_optional_uuid(field: &str, raw: Option<String>) -> Result<Option<Uuid>, Status> {
    match raw {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            Ok(Some(parse_uuid(field, trimmed)?))
        }
        None => Ok(None),
    }
}

fn parse_decimal(field: &str, raw: &str) -> Result<Decimal, Status> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument(format!("{field} is required")));
    }
    Decimal::from_str(trimmed)
        .map_err(|e| Status::invalid_argument(format!("invalid decimal for {field}: {e}")))
}

fn parse_optional_ip(raw: Option<String>) -> Result<Option<IpAddr>, Status> {
    match raw {
        Some(value) => {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                return Ok(None);
            }
            let parsed = IpAddr::from_str(trimmed)
                .map_err(|e| Status::invalid_argument(format!("invalid client_ip: {e}")))?;
            Ok(Some(parsed))
        }
        None => Ok(None),
    }
}

fn parse_optional_family(raw: Option<String>) -> Result<Option<String>, Status> {
    match raw {
        Some(value) => {
            let normalized = value.trim().to_ascii_lowercase();
            if normalized.is_empty() {
                return Ok(None);
            }
            if normalized.len() > 50 {
                return Err(Status::invalid_argument("family max length is 50"));
            }
            if !normalized
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            {
                return Err(Status::invalid_argument(
                    "family contains invalid characters",
                ));
            }
            Ok(Some(normalized))
        }
        None => Ok(None),
    }
}

fn parse_timestamp(field: &str, raw: &str) -> Result<DateTime<Utc>, Status> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument(format!("{field} is required")));
    }
    DateTime::parse_from_rfc3339(trimmed)
        .map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
        .map(|dt| dt.with_timezone(&Utc))
}

fn parse_json(field: &str, raw: &str) -> Result<Value, Status> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(Status::invalid_argument(format!("{field} is required")));
    }
    serde_json::from_str(trimmed)
        .map_err(|e| Status::invalid_argument(format!("invalid JSON for {field}: {e}")))
}

fn parse_optional_timestamp(
    field: &str,
    raw: Option<String>,
) -> Result<Option<DateTime<Utc>>, Status> {
    match raw {
        Some(value) if !value.trim().is_empty() => Ok(Some(parse_timestamp(field, &value)?)),
        Some(_) | None => Ok(None),
    }
}

fn claims_from_request<T>(request: &Request<T>) -> Result<ServiceJwtClaims, Status> {
    request
        .extensions()
        .get::<ServiceJwtClaims>()
        .cloned()
        .ok_or_else(|| Status::unauthenticated("missing service claims"))
}

fn require_admin_context(
    admin_callers: &HashSet<String>,
    claims: &ServiceJwtClaims,
    admin_actor_id: &str,
    client_ip: Option<String>,
) -> Result<AuditContext, Status> {
    if !admin_callers.contains(&claims.sub) {
        return Err(Status::permission_denied(format!(
            "caller '{}' is not allowed for admin operations",
            claims.sub
        )));
    }

    let actor_id = parse_uuid("admin_actor_id", admin_actor_id)?;
    let ip_address = parse_optional_ip(client_ip)?;

    Ok(AuditContext {
        actor_type: AuditActorType::Admin,
        actor_id: Some(actor_id),
        ip_address,
    })
}

impl CatalogGrpcService {
    fn is_admin_caller(&self, claims: &ServiceJwtClaims) -> bool {
        self.admin_callers.contains(&claims.sub)
    }

    fn plan_id_for_visibility_check(
        &self,
        claims: &ServiceJwtClaims,
        plan_id: Option<String>,
    ) -> Result<Option<Uuid>, Status> {
        let parsed = parse_optional_uuid("plan_id", plan_id)?;
        if self.is_admin_caller(claims) {
            return Ok(parsed);
        }

        if parsed.is_none() {
            return Err(Status::invalid_argument(
                "plan_id is required for non-admin callers",
            ));
        }
        Ok(parsed)
    }

    async fn ensure_model_visible_to_caller(
        &self,
        claims: &ServiceJwtClaims,
        model: &ModelCatalog,
        plan_id: Option<String>,
    ) -> Result<(), Status> {
        if self.is_admin_caller(claims) {
            return Ok(());
        }

        let plan_id = self
            .plan_id_for_visibility_check(claims, plan_id)?
            .ok_or_else(|| Status::invalid_argument("plan_id is required for non-admin callers"))?;

        if !model.is_active {
            return Err(Status::not_found("model not found"));
        }

        let has_access = self
            .get_model
            .repo
            .has_plan_access(plan_id, model.id)
            .await
            .map_err(map_error)?;
        if !has_access {
            return Err(Status::not_found("model not found"));
        }

        Ok(())
    }

    fn require_admin_caller(
        &self,
        claims: &ServiceJwtClaims,
        operation: &str,
    ) -> Result<(), Status> {
        if self.is_admin_caller(claims) {
            return Ok(());
        }

        Err(Status::permission_denied(format!(
            "caller '{}' is not allowed for {operation}",
            claims.sub
        )))
    }
}

fn to_proto_model(model: &ModelCatalog) -> Model {
    Model {
        id: model.id.to_string(),
        provider_id: model.provider_id.to_string(),
        public_model_name: model.public_model_name.clone(),
        provider_model_id: model.provider_model_id.clone(),
        family: model.family.clone(),
        context_window: model.context_window,
        max_output_tokens: model.max_output_tokens,
        supports_streaming: model.supports_streaming,
        supports_tools: model.supports_tools,
        supports_vision: model.supports_vision,
        is_active: model.is_active,
        deprecated_at: model.deprecated_at.map(|dt| dt.to_rfc3339()),
        created_at: model.created_at.to_rfc3339(),
    }
}

fn to_proto_provider(provider: &ModelProvider) -> Provider {
    Provider {
        id: provider.id.to_string(),
        name: provider.name.clone(),
        is_active: provider.is_active,
        contact_email: provider.contact_email.clone(),
    }
}

fn to_proto_metadata(metadata: &ModelMetadata) -> ProtoModelMetadata {
    ProtoModelMetadata {
        id: metadata.id.to_string(),
        model_catalog_id: metadata.model_catalog_id.to_string(),
        serving_provider_id: metadata.serving_provider_id.map(|value| value.to_string()),
        description: metadata.description.clone(),
        architecture: metadata.architecture.clone(),
        parameter_count: metadata.parameter_count,
        precision: metadata.precision.clone(),
        quantization: metadata.quantization.clone(),
        metadata_json: metadata.metadata.to_string(),
        source_url: metadata.source_url.clone(),
        source_name: metadata.source_name.clone(),
        source_updated_at: metadata.source_updated_at.map(|value| value.to_rfc3339()),
        metadata_version: metadata.metadata_version,
        created_at: metadata.created_at.to_rfc3339(),
        updated_at: metadata.updated_at.to_rfc3339(),
    }
}

fn to_proto_policy(policy: &ProviderPolicy) -> ProtoProviderPolicy {
    ProtoProviderPolicy {
        id: policy.id.to_string(),
        provider_id: policy.provider_id.to_string(),
        model_catalog_id: policy.model_catalog_id.map(|value| value.to_string()),
        policy_scope: policy.policy_scope.clone(),
        retention_mode: policy.retention_mode.clone(),
        retention_days: policy.retention_days,
        training_usage: policy.training_usage.clone(),
        provider_access: policy.provider_access.clone(),
        supported_retention_modes_json: policy.supported_retention_modes.to_string(),
        data_residency_json: policy.data_residency.to_string(),
        security_controls_json: policy.security_controls.to_string(),
        compliance_json: policy.compliance.to_string(),
        limitations_json: policy.limitations.to_string(),
        privacy_policy_url: policy.privacy_policy_url.clone(),
        terms_url: policy.terms_url.clone(),
        documentation_url: policy.documentation_url.clone(),
        source_url: policy.source_url.clone(),
        policy_version: policy.policy_version.clone(),
        effective_from: policy.effective_from.to_rfc3339(),
        effective_to: policy.effective_to.map(|value| value.to_rfc3339()),
        is_current: policy.is_current,
        created_by_admin_id: policy.created_by_admin_id.map(|value| value.to_string()),
        created_at: policy.created_at.to_rfc3339(),
    }
}

fn to_proto_pricing(pricing: &ModelPricing) -> ProtoModelPricing {
    ProtoModelPricing {
        id: pricing.id.to_string(),
        model_catalog_id: pricing.model_catalog_id.to_string(),
        aws_cost_per_1k_input: pricing.aws_cost_per_1k_input.to_string(),
        aws_cost_per_1k_output: pricing.aws_cost_per_1k_output.to_string(),
        price_per_1k_input: pricing.price_per_1k_input.to_string(),
        price_per_1k_output: pricing.price_per_1k_output.to_string(),
        effective_from: pricing.effective_from.to_rfc3339(),
        effective_to: pricing.effective_to.map(|dt| dt.to_rfc3339()),
        created_by_admin_id: pricing.created_by_admin_id.map(|id| id.to_string()),
    }
}

#[tonic::async_trait]
impl CatalogService for CatalogGrpcService {
    async fn health_check(
        &self,
        _request: Request<HealthCheckRequest>,
    ) -> Result<Response<HealthCheckResponse>, Status> {
        match self.list_providers.execute().await {
            Ok(_) => Ok(Response::new(HealthCheckResponse {
                ok: true,
                status: "ok".to_string(),
            })),
            Err(error) => Ok(Response::new(HealthCheckResponse {
                ok: false,
                status: format!("degraded: {}", error),
            })),
        }
    }

    async fn list_models(
        &self,
        request: Request<ListModelsRequest>,
    ) -> Result<Response<ListModelsResponse>, Status> {
        let claims = claims_from_request(&request)?;
        let req = request.into_inner();

        let is_admin = self.is_admin_caller(&claims);
        let plan_id = self.plan_id_for_visibility_check(&claims, req.plan_id)?;

        let limit = req
            .limit
            .ok_or_else(|| Status::invalid_argument("limit is required"))?;

        let filter = ModelQueryFilter {
            plan_id,
            provider_id: parse_optional_uuid("provider_id", req.provider_id)?,
            family: parse_optional_family(req.family)?,
            active: if is_admin { req.active } else { Some(true) },
            limit: Some(limit),
            after_cursor: req.after_cursor,
        };

        let (models, next_cursor) = self.list_models.execute(filter).await.map_err(map_error)?;
        Ok(Response::new(ListModelsResponse {
            models: models.iter().map(to_proto_model).collect(),
            next_cursor: next_cursor.unwrap_or_default(),
        }))
    }

    async fn get_model(
        &self,
        request: Request<GetModelRequest>,
    ) -> Result<Response<GetModelResponse>, Status> {
        let claims = claims_from_request(&request)?;
        let req = request.into_inner();
        let model_id = parse_uuid("model_id", &req.model_id)?;

        let model = self
            .get_model
            .execute(model_id)
            .await
            .map_err(map_error)?
            .ok_or_else(|| Status::not_found("model not found"))?;
        self.ensure_model_visible_to_caller(&claims, &model, req.plan_id)
            .await?;

        Ok(Response::new(GetModelResponse {
            model: Some(to_proto_model(&model)),
        }))
    }

    async fn get_model_by_public_name(
        &self,
        request: Request<GetModelByPublicNameRequest>,
    ) -> Result<Response<GetModelByPublicNameResponse>, Status> {
        let claims = claims_from_request(&request)?;
        let req = request.into_inner();
        let name = req.public_model_name.trim();
        if name.is_empty() {
            return Err(Status::invalid_argument("public_model_name is required"));
        }

        let model = self
            .get_by_name
            .execute(name)
            .await
            .map_err(map_error)?
            .ok_or_else(|| Status::not_found("model not found"))?;
        self.ensure_model_visible_to_caller(&claims, &model, req.plan_id)
            .await?;

        Ok(Response::new(GetModelByPublicNameResponse {
            model: Some(to_proto_model(&model)),
        }))
    }

    async fn list_providers(
        &self,
        _request: Request<ListProvidersRequest>,
    ) -> Result<Response<ListProvidersResponse>, Status> {
        let providers = self.list_providers.execute().await.map_err(map_error)?;
        Ok(Response::new(ListProvidersResponse {
            providers: providers.iter().map(to_proto_provider).collect(),
        }))
    }

    async fn get_current_pricing(
        &self,
        request: Request<GetCurrentPricingRequest>,
    ) -> Result<Response<GetCurrentPricingResponse>, Status> {
        let claims = claims_from_request(&request)?;
        let req = request.into_inner();
        let model_catalog_id = parse_uuid("model_catalog_id", &req.model_catalog_id)?;
        let model = self
            .get_model
            .execute(model_catalog_id)
            .await
            .map_err(map_error)?
            .ok_or_else(|| Status::not_found("model not found"))?;
        self.ensure_model_visible_to_caller(&claims, &model, req.plan_id)
            .await?;

        let pricing = self
            .get_pricing
            .execute(model_catalog_id)
            .await
            .map_err(map_error)?
            .ok_or_else(|| Status::not_found("active pricing not found"))?;

        Ok(Response::new(GetCurrentPricingResponse {
            pricing: Some(to_proto_pricing(&pricing)),
        }))
    }

    async fn get_pricing_at(
        &self,
        request: Request<GetPricingAtRequest>,
    ) -> Result<Response<GetPricingAtResponse>, Status> {
        let claims = claims_from_request(&request)?;
        self.require_admin_caller(&claims, "pricing history queries")?;
        let req = request.into_inner();
        let model_catalog_id = parse_uuid("model_catalog_id", &req.model_catalog_id)?;
        let timestamp = parse_timestamp("timestamp", &req.timestamp)?;

        let pricing = self
            .get_pricing_at
            .execute(model_catalog_id, timestamp)
            .await
            .map_err(map_error)?
            .ok_or_else(|| Status::not_found("pricing not found at timestamp"))?;

        Ok(Response::new(GetPricingAtResponse {
            pricing: Some(to_proto_pricing(&pricing)),
        }))
    }

    async fn list_pricing_history(
        &self,
        request: Request<ListPricingHistoryRequest>,
    ) -> Result<Response<ListPricingHistoryResponse>, Status> {
        let claims = claims_from_request(&request)?;
        self.require_admin_caller(&claims, "pricing history queries")?;
        let req = request.into_inner();
        let model_catalog_id = parse_uuid("model_catalog_id", &req.model_catalog_id)?;

        let pricing = self
            .list_pricing_history
            .execute(model_catalog_id)
            .await
            .map_err(map_error)?;

        Ok(Response::new(ListPricingHistoryResponse {
            pricing: pricing.iter().map(to_proto_pricing).collect(),
        }))
    }

    async fn register_model_provider(
        &self,
        request: Request<RegisterModelProviderRequest>,
    ) -> Result<Response<RegisterModelProviderResponse>, Status> {
        let claims = claims_from_request(&request)?;
        let req = request.into_inner();

        let audit_context = require_admin_context(
            &self.admin_callers,
            &claims,
            &req.admin_actor_id,
            req.client_ip,
        )?;

        let provider = self
            .register_provider
            .execute(req.name, Some(audit_context))
            .await
            .map_err(map_error)?;

        Ok(Response::new(RegisterModelProviderResponse {
            provider: Some(to_proto_provider(&provider)),
        }))
    }

    async fn register_model(
        &self,
        request: Request<RegisterModelRequest>,
    ) -> Result<Response<RegisterModelResponse>, Status> {
        let claims = claims_from_request(&request)?;
        let req = request.into_inner();

        let audit_context = require_admin_context(
            &self.admin_callers,
            &claims,
            &req.admin_actor_id,
            req.client_ip,
        )?;

        let provider_id = parse_uuid("provider_id", &req.provider_id)?;
        let input = RegisterModelInput {
            provider_id,
            public_model_name: req.public_model_name,
            provider_model_id: req.provider_model_id,
            family: req.family,
            context_window: req.context_window,
            max_output_tokens: req.max_output_tokens,
            capabilities: ModelCapabilities::new(
                req.supports_streaming,
                req.supports_tools,
                req.supports_vision,
            ),
        };

        let model = self
            .register_model
            .execute(input, Some(audit_context))
            .await
            .map_err(map_error)?;

        Ok(Response::new(RegisterModelResponse {
            model: Some(to_proto_model(&model)),
        }))
    }

    async fn update_model_capabilities(
        &self,
        request: Request<UpdateModelCapabilitiesRequest>,
    ) -> Result<Response<UpdateModelCapabilitiesResponse>, Status> {
        let claims = claims_from_request(&request)?;
        let req = request.into_inner();

        let audit_context = require_admin_context(
            &self.admin_callers,
            &claims,
            &req.admin_actor_id,
            req.client_ip,
        )?;

        let model_id = parse_uuid("model_id", &req.model_id)?;
        let model = self
            .update_capabilities
            .execute(
                model_id,
                req.supports_streaming,
                req.supports_tools,
                req.supports_vision,
                req.provider_model_id,
                Some(audit_context),
            )
            .await
            .map_err(map_error)?;

        Ok(Response::new(UpdateModelCapabilitiesResponse {
            model: Some(to_proto_model(&model)),
        }))
    }

    async fn upsert_model_metadata(
        &self,
        request: Request<UpsertModelMetadataRequest>,
    ) -> Result<Response<UpsertModelMetadataResponse>, Status> {
        let claims = claims_from_request(&request)?;
        let req = request.into_inner();
        let audit_context = require_admin_context(
            &self.admin_callers,
            &claims,
            &req.admin_actor_id,
            req.client_ip,
        )?;
        let model_catalog_id = parse_uuid("model_catalog_id", &req.model_catalog_id)?;
        let metadata = self
            .upsert_metadata
            .execute(
                model_catalog_id,
                crate::application::commands::ModelMetadataInput {
                    serving_provider_id: parse_optional_uuid(
                        "serving_provider_id",
                        req.serving_provider_id,
                    )?,
                    description: req.description,
                    architecture: req.architecture,
                    parameter_count: req.parameter_count,
                    precision: req.precision,
                    quantization: req.quantization,
                    metadata: parse_json("metadata_json", &req.metadata_json)?,
                    source_url: req.source_url,
                    source_name: req.source_name,
                    source_updated_at: parse_optional_timestamp(
                        "source_updated_at",
                        req.source_updated_at,
                    )?,
                    metadata_version: req.metadata_version,
                },
                Some(audit_context),
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(UpsertModelMetadataResponse {
            metadata: Some(to_proto_metadata(&metadata)),
        }))
    }

    async fn get_model_metadata(
        &self,
        request: Request<GetModelMetadataRequest>,
    ) -> Result<Response<GetModelMetadataResponse>, Status> {
        let claims = claims_from_request(&request)?;
        self.require_admin_caller(&claims, "model metadata queries")?;
        let req = request.into_inner();
        let model_catalog_id = parse_uuid("model_catalog_id", &req.model_catalog_id)?;
        let metadata = self
            .get_metadata
            .execute(model_catalog_id)
            .await
            .map_err(map_error)?
            .ok_or_else(|| Status::not_found("model metadata not found"))?;
        Ok(Response::new(GetModelMetadataResponse {
            metadata: Some(to_proto_metadata(&metadata)),
        }))
    }

    async fn set_provider_policy(
        &self,
        request: Request<SetProviderPolicyRequest>,
    ) -> Result<Response<SetProviderPolicyResponse>, Status> {
        let claims = claims_from_request(&request)?;
        let req = request.into_inner();
        let audit_context = require_admin_context(
            &self.admin_callers,
            &claims,
            &req.admin_actor_id,
            req.client_ip,
        )?;
        let input = ProviderPolicyInput {
            provider_id: parse_uuid("provider_id", &req.provider_id)?,
            model_catalog_id: parse_optional_uuid("model_catalog_id", req.model_catalog_id)?,
            retention_mode: req.retention_mode,
            retention_days: req.retention_days,
            training_usage: req.training_usage,
            provider_access: req.provider_access,
            supported_retention_modes: parse_json(
                "supported_retention_modes_json",
                &req.supported_retention_modes_json,
            )?,
            data_residency: parse_json("data_residency_json", &req.data_residency_json)?,
            security_controls: parse_json("security_controls_json", &req.security_controls_json)?,
            compliance: parse_json("compliance_json", &req.compliance_json)?,
            limitations: parse_json("limitations_json", &req.limitations_json)?,
            privacy_policy_url: req.privacy_policy_url,
            terms_url: req.terms_url,
            documentation_url: req.documentation_url,
            source_url: req.source_url,
            policy_version: req.policy_version,
        };
        let policy = self
            .set_provider_policy
            .execute(input, Some(audit_context))
            .await
            .map_err(map_error)?;
        Ok(Response::new(SetProviderPolicyResponse {
            policy: Some(to_proto_policy(&policy)),
        }))
    }

    async fn get_provider_policy(
        &self,
        request: Request<GetProviderPolicyRequest>,
    ) -> Result<Response<GetProviderPolicyResponse>, Status> {
        let claims = claims_from_request(&request)?;
        self.require_admin_caller(&claims, "provider policy queries")?;
        let req = request.into_inner();
        let provider_id = parse_uuid("provider_id", &req.provider_id)?;
        let model_catalog_id = parse_optional_uuid("model_catalog_id", req.model_catalog_id)?;
        let policy = self
            .get_provider_policy
            .execute(provider_id, model_catalog_id)
            .await
            .map_err(map_error)?
            .ok_or_else(|| Status::not_found("provider policy not found"))?;
        Ok(Response::new(GetProviderPolicyResponse {
            policy: Some(to_proto_policy(&policy)),
        }))
    }

    async fn deprecate_model(
        &self,
        request: Request<DeprecateModelRequest>,
    ) -> Result<Response<DeprecateModelResponse>, Status> {
        let claims = claims_from_request(&request)?;
        let req = request.into_inner();

        let audit_context = require_admin_context(
            &self.admin_callers,
            &claims,
            &req.admin_actor_id,
            req.client_ip,
        )?;

        let model_id = parse_uuid("model_id", &req.model_id)?;
        self.deprecate_model
            .execute(model_id, Some(audit_context))
            .await
            .map_err(map_error)?;

        Ok(Response::new(DeprecateModelResponse { success: true }))
    }

    async fn set_model_pricing(
        &self,
        request: Request<SetModelPricingRequest>,
    ) -> Result<Response<SetModelPricingResponse>, Status> {
        let claims = claims_from_request(&request)?;
        let req = request.into_inner();

        let audit_context = require_admin_context(
            &self.admin_callers,
            &claims,
            &req.admin_actor_id,
            req.client_ip,
        )?;

        let model_catalog_id = parse_uuid("model_catalog_id", &req.model_catalog_id)?;
        let token_price = TokenPrice::new(
            parse_decimal("aws_cost_per_1k_input", &req.aws_cost_per_1k_input)?,
            parse_decimal("aws_cost_per_1k_output", &req.aws_cost_per_1k_output)?,
            parse_decimal("price_per_1k_input", &req.price_per_1k_input)?,
            parse_decimal("price_per_1k_output", &req.price_per_1k_output)?,
        )
        .map_err(map_error)?;

        let pricing = self
            .set_pricing
            .execute(
                model_catalog_id,
                token_price,
                audit_context.actor_id,
                Some(audit_context),
            )
            .await
            .map_err(map_error)?;

        Ok(Response::new(SetModelPricingResponse {
            pricing: Some(to_proto_pricing(&pricing)),
        }))
    }
}
