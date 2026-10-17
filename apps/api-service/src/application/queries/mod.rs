use crate::domain::entities::{
    ModelCatalog, ModelMetadata, ModelPricing, ModelProvider, ProviderPolicy,
};
use crate::domain::errors::CatalogError;
use crate::domain::repositories::{
    ModelCatalogRepository, ModelPricingRepository, ModelProviderRepository, ModelQueryFilter,
};
use chrono::{DateTime, Utc};
use std::sync::Arc;
use uuid::Uuid;

pub struct ListModels {
    pub repo: Arc<dyn ModelCatalogRepository>,
}

impl ListModels {
    pub async fn execute(
        &self,
        filter: ModelQueryFilter,
    ) -> Result<(Vec<ModelCatalog>, Option<String>), CatalogError> {
        self.repo.find_filtered(&filter).await
    }
}

pub struct GetModel {
    pub repo: Arc<dyn ModelCatalogRepository>,
}

impl GetModel {
    pub async fn execute(&self, id: Uuid) -> Result<Option<ModelCatalog>, CatalogError> {
        self.repo.find_by_id(id).await
    }
}

pub struct GetModelByPublicName {
    pub repo: Arc<dyn ModelCatalogRepository>,
}

impl GetModelByPublicName {
    pub async fn execute(&self, name: &str) -> Result<Option<ModelCatalog>, CatalogError> {
        self.repo.find_by_public_name(name).await
    }
}

pub struct GetCurrentPricing {
    pub repo: Arc<dyn ModelPricingRepository>,
}

impl GetCurrentPricing {
    pub async fn execute(
        &self,
        model_catalog_id: Uuid,
    ) -> Result<Option<ModelPricing>, CatalogError> {
        self.repo.find_active_by_model(model_catalog_id).await
    }
}

pub struct GetPricingAt {
    pub repo: Arc<dyn ModelPricingRepository>,
}

impl GetPricingAt {
    pub async fn execute(
        &self,
        model_catalog_id: Uuid,
        timestamp: DateTime<Utc>,
    ) -> Result<Option<ModelPricing>, CatalogError> {
        self.repo.find_at(model_catalog_id, timestamp).await
    }
}

pub struct ListPricingHistory {
    pub repo: Arc<dyn ModelPricingRepository>,
}

impl ListPricingHistory {
    pub async fn execute(&self, model_catalog_id: Uuid) -> Result<Vec<ModelPricing>, CatalogError> {
        self.repo.find_history(model_catalog_id).await
    }
}

pub struct ListProviders {
    pub repo: Arc<dyn ModelProviderRepository>,
}

impl ListProviders {
    pub async fn execute(&self) -> Result<Vec<ModelProvider>, CatalogError> {
        self.repo.find_all_active().await
    }
}

pub struct GetModelMetadata {
    pub repo: Arc<dyn crate::domain::repositories::ModelMetadataRepository>,
}

impl GetModelMetadata {
    pub async fn execute(
        &self,
        model_catalog_id: Uuid,
    ) -> Result<Option<ModelMetadata>, CatalogError> {
        self.repo.find_by_model(model_catalog_id).await
    }
}

pub struct GetProviderPolicy {
    pub repo: Arc<dyn crate::domain::repositories::ProviderPolicyRepository>,
}

impl GetProviderPolicy {
    pub async fn execute(
        &self,
        provider_id: Uuid,
        model_catalog_id: Option<Uuid>,
    ) -> Result<Option<ProviderPolicy>, CatalogError> {
        match model_catalog_id {
            Some(model_id) => {
                self.repo
                    .find_current_for_model(provider_id, model_id)
                    .await
            }
            None => self.repo.find_current_for_provider(provider_id).await,
        }
    }
}
