use crate::application::ports::{AuditContext, AuditEntry, AuditLogger};
use crate::domain::entities::{
    ModelCapabilities, ModelCatalog, ModelMetadata, ModelMetadataParams, ModelPricing,
    ModelProvider, ProviderPolicy, ProviderPolicyInput, TokenPrice,
};
use crate::domain::errors::CatalogError;
use crate::domain::repositories::{
    ModelCatalogRepository, ModelPricingRepository, ModelProviderRepository,
};
use serde_json::json;
use std::sync::Arc;
use tracing::{error, info};
use uuid::Uuid;

fn map_provider_save_error(error: CatalogError, provider_name: &str) -> CatalogError {
    match error {
        CatalogError::Validation(message) if message.contains("model_providers_name_key") => {
            CatalogError::ProviderNameAlreadyExists {
                name: provider_name.to_string(),
            }
        }
        other => other,
    }
}

fn map_model_save_error(error: CatalogError, public_model_name: &str) -> CatalogError {
    match error {
        CatalogError::Validation(message)
            if message.contains("model_catalog_public_model_name_key") =>
        {
            CatalogError::ModelNameAlreadyExists {
                name: public_model_name.to_string(),
            }
        }
        other => other,
    }
}

pub struct RegisterModelProvider {
    pub repo: Arc<dyn ModelProviderRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

impl RegisterModelProvider {
    pub async fn execute(
        &self,
        name: String,
        audit_context: Option<AuditContext>,
    ) -> Result<ModelProvider, CatalogError> {
        let provider = ModelProvider::new(name)?;

        if self.repo.find_by_name(&provider.name).await?.is_some() {
            return Err(CatalogError::ProviderNameAlreadyExists {
                name: provider.name.clone(),
            });
        }

        self.repo
            .save(&provider)
            .await
            .map_err(|error| map_provider_save_error(error, &provider.name))?;
        info!(provider_id = %provider.id, "Model provider registered");

        if let Some(ctx) = audit_context {
            self.record_audit(
                &ctx,
                "catalog.provider.created",
                "model_provider",
                Some(provider.id),
                json!({ "name": provider.name }),
            )
            .await;
        }

        Ok(provider)
    }

    async fn record_audit(
        &self,
        context: &AuditContext,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
        metadata: serde_json::Value,
    ) {
        if let Err(e) = self
            .audit
            .record(AuditEntry {
                actor_type: context.actor_type.clone(),
                actor_id: context.actor_id,
                action: action.to_string(),
                resource_type: resource_type.to_string(),
                resource_id,
                metadata,
                ip_address: context.ip_address,
            })
            .await
        {
            error!(error = %e, action, resource_type, "Failed to write audit log");
        }
    }
}

pub struct RegisterModel {
    pub provider_repo: Arc<dyn ModelProviderRepository>,
    pub model_repo: Arc<dyn ModelCatalogRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

pub struct RegisterModelInput {
    pub provider_id: Uuid,
    pub public_model_name: String,
    pub provider_model_id: String,
    pub family: Option<String>,
    pub context_window: i32,
    pub max_output_tokens: i32,
    pub capabilities: ModelCapabilities,
}

impl RegisterModel {
    pub async fn execute(
        &self,
        input: RegisterModelInput,
        audit_context: Option<AuditContext>,
    ) -> Result<ModelCatalog, CatalogError> {
        let RegisterModelInput {
            provider_id,
            public_model_name,
            provider_model_id,
            family,
            context_window,
            max_output_tokens,
            capabilities,
        } = input;

        let provider = self
            .provider_repo
            .find_by_id(provider_id)
            .await?
            .ok_or_else(|| CatalogError::ProviderNotFound {
                provider_id: provider_id.to_string(),
            })?;

        if !provider.is_active {
            return Err(CatalogError::ProviderInactive {
                provider_id: provider.id.to_string(),
            });
        }

        let model = ModelCatalog::new(
            provider_id,
            public_model_name,
            provider_model_id,
            family,
            context_window,
            max_output_tokens,
            capabilities,
        )?;

        if self
            .model_repo
            .find_by_public_name(&model.public_model_name)
            .await?
            .is_some()
        {
            return Err(CatalogError::ModelNameAlreadyExists {
                name: model.public_model_name.clone(),
            });
        }

        self.model_repo
            .save(&model)
            .await
            .map_err(|error| map_model_save_error(error, &model.public_model_name))?;
        info!(model_id = %model.id, "Model registered");

        if let Some(ctx) = audit_context {
            self.record_audit(
                &ctx,
                "catalog.model.created",
                "model_catalog",
                Some(model.id),
                json!({
                    "provider_id": model.provider_id,
                    "public_model_name": model.public_model_name,
                    "provider_model_id": model.provider_model_id,
                    "family": model.family,
                    "context_window": model.context_window,
                    "max_output_tokens": model.max_output_tokens,
                    "supports_streaming": model.supports_streaming,
                    "supports_tools": model.supports_tools,
                    "supports_vision": model.supports_vision
                }),
            )
            .await;
        }

        Ok(model)
    }

    async fn record_audit(
        &self,
        context: &AuditContext,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
        metadata: serde_json::Value,
    ) {
        if let Err(e) = self
            .audit
            .record(AuditEntry {
                actor_type: context.actor_type.clone(),
                actor_id: context.actor_id,
                action: action.to_string(),
                resource_type: resource_type.to_string(),
                resource_id,
                metadata,
                ip_address: context.ip_address,
            })
            .await
        {
            error!(error = %e, action, resource_type, "Failed to write audit log");
        }
    }
}

pub struct DeprecateModel {
    pub repo: Arc<dyn ModelCatalogRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

impl DeprecateModel {
    pub async fn execute(
        &self,
        model_id: Uuid,
        audit_context: Option<AuditContext>,
    ) -> Result<(), CatalogError> {
        let mut model =
            self.repo
                .find_by_id(model_id)
                .await?
                .ok_or_else(|| CatalogError::ModelNotFound {
                    model_id: model_id.to_string(),
                })?;

        model.deprecate(chrono::Utc::now());
        self.repo.update(&model).await?;
        info!(model_id = %model_id, "Model deprecated");

        if let Some(ctx) = audit_context {
            self.record_audit(
                &ctx,
                "catalog.model.deprecated",
                "model_catalog",
                Some(model_id),
                json!({ "deprecated_at": model.deprecated_at.map(|dt| dt.to_rfc3339()) }),
            )
            .await;
        }

        Ok(())
    }

    async fn record_audit(
        &self,
        context: &AuditContext,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
        metadata: serde_json::Value,
    ) {
        if let Err(e) = self
            .audit
            .record(AuditEntry {
                actor_type: context.actor_type.clone(),
                actor_id: context.actor_id,
                action: action.to_string(),
                resource_type: resource_type.to_string(),
                resource_id,
                metadata,
                ip_address: context.ip_address,
            })
            .await
        {
            error!(error = %e, action, resource_type, "Failed to write audit log");
        }
    }
}

pub struct SetModelPricing {
    pub model_repo: Arc<dyn ModelCatalogRepository>,
    pub pricing_repo: Arc<dyn ModelPricingRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

impl SetModelPricing {
    pub async fn execute(
        &self,
        model_catalog_id: Uuid,
        token_price: TokenPrice,
        admin_id: Option<Uuid>,
        audit_context: Option<AuditContext>,
    ) -> Result<ModelPricing, CatalogError> {
        let model = self
            .model_repo
            .find_by_id(model_catalog_id)
            .await?
            .ok_or_else(|| CatalogError::ModelNotFound {
                model_id: model_catalog_id.to_string(),
            })?;

        if !model.is_active {
            return Err(CatalogError::ModelDeprecated {
                model_id: model_catalog_id.to_string(),
            });
        }

        let pricing = ModelPricing::new(model_catalog_id, token_price, admin_id)?;
        self.pricing_repo.replace_active_pricing(&pricing).await?;

        info!(pricing_id = %pricing.id, model_id = %model_catalog_id, "Pricing set");

        if let Some(ctx) = audit_context {
            self.record_audit(
                &ctx,
                "catalog.model_pricing.set",
                "model_pricing",
                Some(pricing.id),
                json!({
                    "model_catalog_id": pricing.model_catalog_id,
                    "aws_cost_per_1k_input": pricing.aws_cost_per_1k_input,
                    "aws_cost_per_1k_output": pricing.aws_cost_per_1k_output,
                    "price_per_1k_input": pricing.price_per_1k_input,
                    "price_per_1k_output": pricing.price_per_1k_output,
                    "effective_from": pricing.effective_from.to_rfc3339()
                }),
            )
            .await;
        }

        Ok(pricing)
    }

    async fn record_audit(
        &self,
        context: &AuditContext,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
        metadata: serde_json::Value,
    ) {
        if let Err(e) = self
            .audit
            .record(AuditEntry {
                actor_type: context.actor_type.clone(),
                actor_id: context.actor_id,
                action: action.to_string(),
                resource_type: resource_type.to_string(),
                resource_id,
                metadata,
                ip_address: context.ip_address,
            })
            .await
        {
            error!(error = %e, action, resource_type, "Failed to write audit log");
        }
    }
}

pub struct UpdateModelCapabilities {
    pub repo: Arc<dyn ModelCatalogRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

pub struct UpsertModelMetadata {
    pub model_repo: Arc<dyn ModelCatalogRepository>,
    pub metadata_repo: Arc<dyn crate::domain::repositories::ModelMetadataRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

impl UpsertModelMetadata {
    pub async fn execute(
        &self,
        model_catalog_id: Uuid,
        input: ModelMetadataInput,
        audit_context: Option<AuditContext>,
    ) -> Result<ModelMetadata, CatalogError> {
        self.model_repo
            .find_by_id(model_catalog_id)
            .await?
            .ok_or_else(|| CatalogError::ModelNotFound {
                model_id: model_catalog_id.to_string(),
            })?;
        let metadata = ModelMetadata::new(ModelMetadataParams {
            model_catalog_id,
            serving_provider_id: input.serving_provider_id,
            description: input.description,
            architecture: input.architecture,
            parameter_count: input.parameter_count,
            precision: input.precision,
            quantization: input.quantization,
            metadata: input.metadata,
            source_url: input.source_url,
            source_name: input.source_name,
            source_updated_at: input.source_updated_at,
            metadata_version: input.metadata_version,
        })?;
        self.metadata_repo.save_or_replace(&metadata).await?;
        if let Some(context) = audit_context {
            if let Err(error) = self
                .audit
                .record(AuditEntry {
                    actor_type: context.actor_type,
                    actor_id: context.actor_id,
                    action: "catalog.model_metadata.upserted".to_string(),
                    resource_type: "model_metadata".to_string(),
                    resource_id: Some(model_catalog_id),
                    metadata: json!({
                        "metadata_version": metadata.metadata_version,
                        "architecture": metadata.architecture,
                        "parameter_count": metadata.parameter_count,
                        "precision": metadata.precision
                    }),
                    ip_address: context.ip_address,
                })
                .await
            {
                error!(error = %error, "Failed to write model metadata audit log");
            }
        }
        Ok(metadata)
    }
}

#[derive(Debug, Clone)]
pub struct ModelMetadataInput {
    pub serving_provider_id: Option<Uuid>,
    pub description: Option<String>,
    pub architecture: Option<String>,
    pub parameter_count: Option<i64>,
    pub precision: Option<String>,
    pub quantization: Option<String>,
    pub metadata: serde_json::Value,
    pub source_url: Option<String>,
    pub source_name: Option<String>,
    pub source_updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub metadata_version: i32,
}

pub struct SetProviderPolicy {
    pub provider_repo: Arc<dyn ModelProviderRepository>,
    pub model_repo: Arc<dyn ModelCatalogRepository>,
    pub policy_repo: Arc<dyn crate::domain::repositories::ProviderPolicyRepository>,
    pub audit: Arc<dyn AuditLogger>,
}

impl SetProviderPolicy {
    pub async fn execute(
        &self,
        input: ProviderPolicyInput,
        audit_context: Option<AuditContext>,
    ) -> Result<ProviderPolicy, CatalogError> {
        let provider = self
            .provider_repo
            .find_by_id(input.provider_id)
            .await?
            .ok_or_else(|| CatalogError::ProviderNotFound {
                provider_id: input.provider_id.to_string(),
            })?;
        if !provider.is_active {
            return Err(CatalogError::ProviderInactive {
                provider_id: provider.id.to_string(),
            });
        }
        if let Some(model_catalog_id) = input.model_catalog_id {
            let model = self
                .model_repo
                .find_by_id(model_catalog_id)
                .await?
                .ok_or_else(|| CatalogError::ModelNotFound {
                    model_id: model_catalog_id.to_string(),
                })?;
            if model.provider_id != provider.id {
                return Err(CatalogError::Validation(
                    "model does not belong to provider".to_string(),
                ));
            }
        }
        let policy = ProviderPolicy::new(
            input,
            audit_context.as_ref().and_then(|value| value.actor_id),
        )?;
        self.policy_repo.replace_current(&policy).await?;
        if let Some(context) = audit_context {
            if let Err(error) = self
                .audit
                .record(AuditEntry {
                    actor_type: context.actor_type,
                    actor_id: context.actor_id,
                    action: "catalog.provider_policy.replaced".to_string(),
                    resource_type: "provider_policy".to_string(),
                    resource_id: Some(policy.id),
                    metadata: json!({
                        "provider_id": policy.provider_id,
                        "model_catalog_id": policy.model_catalog_id,
                        "policy_version": policy.policy_version,
                        "retention_mode": policy.retention_mode
                    }),
                    ip_address: context.ip_address,
                })
                .await
            {
                error!(error = %error, "Failed to write provider policy audit log");
            }
        }
        Ok(policy)
    }
}

impl UpdateModelCapabilities {
    pub async fn execute(
        &self,
        model_id: Uuid,
        supports_streaming: Option<bool>,
        supports_tools: Option<bool>,
        supports_vision: Option<bool>,
        provider_model_id: Option<String>,
        audit_context: Option<AuditContext>,
    ) -> Result<ModelCatalog, CatalogError> {
        let mut model =
            self.repo
                .find_by_id(model_id)
                .await?
                .ok_or_else(|| CatalogError::ModelNotFound {
                    model_id: model_id.to_string(),
                })?;

        let new_supports_streaming = match supports_streaming {
            Some(value) => value,
            None => model.supports_streaming,
        };
        let new_supports_tools = match supports_tools {
            Some(value) => value,
            None => model.supports_tools,
        };
        let new_supports_vision = match supports_vision {
            Some(value) => value,
            None => model.supports_vision,
        };
        let capabilities = ModelCapabilities::new(
            new_supports_streaming,
            new_supports_tools,
            new_supports_vision,
        );
        model.set_capabilities(capabilities);
        if let Some(provider_model_id) = provider_model_id {
            model.set_provider_model_id(provider_model_id)?;
        }

        self.repo.update(&model).await?;

        if let Some(ctx) = audit_context {
            self.record_audit(
                &ctx,
                "catalog.model.capabilities.updated",
                "model_catalog",
                Some(model.id),
                json!({
                    "supports_streaming": model.supports_streaming,
                    "supports_tools": model.supports_tools,
                    "supports_vision": model.supports_vision
                    ,"provider_model_id": model.provider_model_id
                }),
            )
            .await;
        }

        Ok(model)
    }

    async fn record_audit(
        &self,
        context: &AuditContext,
        action: &str,
        resource_type: &str,
        resource_id: Option<Uuid>,
        metadata: serde_json::Value,
    ) {
        if let Err(e) = self
            .audit
            .record(AuditEntry {
                actor_type: context.actor_type.clone(),
                actor_id: context.actor_id,
                action: action.to_string(),
                resource_type: resource_type.to_string(),
                resource_id,
                metadata,
                ip_address: context.ip_address,
            })
            .await
        {
            error!(error = %e, action, resource_type, "Failed to write audit log");
        }
    }
}
