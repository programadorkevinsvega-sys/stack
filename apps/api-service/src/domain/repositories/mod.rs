use super::entities::{ModelCatalog, ModelMetadata, ModelPricing, ModelProvider, ProviderPolicy};
use super::errors::CatalogError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub struct ModelQueryFilter {
    pub plan_id: Option<Uuid>,
    pub provider_id: Option<Uuid>,
    pub family: Option<String>,
    pub active: Option<bool>,
    pub limit: Option<u32>,
    /// Keyset cursor: opaque string from previous page's next_cursor.
    /// When set, overrides offset-based pagination with keyset-based WHERE.
    pub after_cursor: Option<String>,
}

#[async_trait]
pub trait ModelProviderRepository: Send + Sync {
    async fn save(&self, provider: &ModelProvider) -> Result<(), CatalogError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<ModelProvider>, CatalogError>;
    async fn find_by_name(&self, name: &str) -> Result<Option<ModelProvider>, CatalogError>;
    async fn find_all_active(&self) -> Result<Vec<ModelProvider>, CatalogError>;
}

#[async_trait]
pub trait ModelCatalogRepository: Send + Sync {
    async fn save(&self, model: &ModelCatalog) -> Result<(), CatalogError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<ModelCatalog>, CatalogError>;
    async fn find_by_public_name(&self, name: &str) -> Result<Option<ModelCatalog>, CatalogError>;
    async fn has_plan_access(&self, plan_id: Uuid, model_id: Uuid) -> Result<bool, CatalogError>;
    async fn find_filtered(
        &self,
        filter: &ModelQueryFilter,
    ) -> Result<(Vec<ModelCatalog>, Option<String>), CatalogError>;
    async fn find_all_active(&self) -> Result<Vec<ModelCatalog>, CatalogError>;
    async fn update(&self, model: &ModelCatalog) -> Result<(), CatalogError>;
}

#[async_trait]
pub trait ModelPricingRepository: Send + Sync {
    async fn save(&self, pricing: &ModelPricing) -> Result<(), CatalogError>;
    async fn replace_active_pricing(&self, pricing: &ModelPricing) -> Result<(), CatalogError>;
    async fn close_active_pricing(&self, model_catalog_id: Uuid) -> Result<(), CatalogError>;
    async fn find_active_by_model(
        &self,
        model_catalog_id: Uuid,
    ) -> Result<Option<ModelPricing>, CatalogError>;
    async fn find_at(
        &self,
        model_catalog_id: Uuid,
        timestamp: DateTime<Utc>,
    ) -> Result<Option<ModelPricing>, CatalogError>;
    async fn find_history(&self, model_catalog_id: Uuid)
        -> Result<Vec<ModelPricing>, CatalogError>;
}

#[async_trait]
pub trait ModelMetadataRepository: Send + Sync {
    async fn save_or_replace(&self, metadata: &ModelMetadata) -> Result<(), CatalogError>;
    async fn find_by_model(
        &self,
        model_catalog_id: Uuid,
    ) -> Result<Option<ModelMetadata>, CatalogError>;
}

#[async_trait]
pub trait ProviderPolicyRepository: Send + Sync {
    async fn replace_current(&self, policy: &ProviderPolicy) -> Result<(), CatalogError>;
    async fn find_current_for_provider(
        &self,
        provider_id: Uuid,
    ) -> Result<Option<ProviderPolicy>, CatalogError>;
    async fn find_current_for_model(
        &self,
        provider_id: Uuid,
        model_catalog_id: Uuid,
    ) -> Result<Option<ProviderPolicy>, CatalogError>;
}
