use crate::domain::entities::{Plan, PlanLimits, Subscription};
use crate::domain::errors::SubscriptionError;
use async_trait::async_trait;
use uuid::Uuid;
#[async_trait]
pub trait PlanRepository: Send + Sync {
    async fn save(&self, plan: &Plan) -> Result<(), SubscriptionError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Plan>, SubscriptionError>;
    async fn find_all_active(&self) -> Result<Vec<Plan>, SubscriptionError>;
    async fn update(&self, plan: &Plan) -> Result<(), SubscriptionError>;
    async fn accept_terms(
        &self,
        user_id: Uuid,
        terms_version: &str,
        privacy_version: &str,
        ip_address: Option<&str>,
        user_agent: &str,
        country: Option<&str>,
    ) -> Result<chrono::DateTime<chrono::Utc>, SubscriptionError> {
        let _ = (
            user_id,
            terms_version,
            privacy_version,
            ip_address,
            user_agent,
            country,
        );
        Err(SubscriptionError::DependencyUnavailable(
            "legal acceptance repository unavailable".into(),
        ))
    }
    async fn has_terms_acceptance(
        &self,
        user_id: Uuid,
        terms_version: &str,
        privacy_version: &str,
    ) -> Result<bool, SubscriptionError> {
        let _ = (user_id, terms_version, privacy_version);
        Ok(false)
    }
}
#[async_trait]
pub trait SubscriptionRepository: Send + Sync {
    async fn save(&self, subscription: &Subscription) -> Result<(), SubscriptionError>;
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Subscription>, SubscriptionError>;
    async fn find_active_by_user(
        &self,
        user_id: Uuid,
    ) -> Result<Option<Subscription>, SubscriptionError>;
    async fn find_current_by_user(
        &self,
        user_id: Uuid,
    ) -> Result<Option<Subscription>, SubscriptionError>;
    async fn update(&self, subscription: &Subscription) -> Result<(), SubscriptionError>;
}
#[async_trait]
pub trait PlanLimitsCache: Send + Sync {
    async fn get(&self, user_id: Uuid) -> Result<Option<PlanLimits>, SubscriptionError>;
    async fn set(&self, user_id: Uuid, limits: &PlanLimits) -> Result<(), SubscriptionError>;
    async fn invalidate(&self, user_id: Uuid) -> Result<(), SubscriptionError>;
    async fn invalidate_plan(&self, plan_id: Uuid) -> Result<(), SubscriptionError> {
        let _ = plan_id;
        Ok(())
    }
}
