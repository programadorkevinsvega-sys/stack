use crate::domain::entities::{Plan, PlanLimits, Subscription};
use crate::domain::errors::SubscriptionError;
use crate::domain::repositories::{PlanLimitsCache, PlanRepository, SubscriptionRepository};
use std::sync::Arc;
use uuid::Uuid;

fn effective_qdrant_mode(plan: &Plan) -> String {
    if plan.qdrant_mode == "none" {
        return "none".to_owned();
    }
    let dedicated_enabled = plan
        .features
        .get("dedicated_qdrant_enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if dedicated_enabled {
        "dedicated".to_owned()
    } else {
        match plan.qdrant_mode.as_str() {
            "shared" => "shared".to_owned(),
            _ => "none".to_owned(),
        }
    }
}
pub struct GetPlanLimits {
    pub subscriptions: Arc<dyn SubscriptionRepository>,
    pub plans: Arc<dyn PlanRepository>,
    pub cache: Arc<dyn PlanLimitsCache>,
}
impl GetPlanLimits {
    pub async fn execute(&self, user_id: Uuid) -> Result<Option<PlanLimits>, SubscriptionError> {
        if let Some(value) = self.cache.get(user_id).await? {
            return Ok(Some(value));
        };
        let subscription = match self.subscriptions.find_active_by_user(user_id).await? {
            Some(v) => v,
            None => return Ok(None),
        };
        let plan = self
            .plans
            .find_by_id(subscription.plan_id)
            .await?
            .filter(|v| v.is_active)
            .ok_or(SubscriptionError::PlanNotFound)?;
        let qdrant_mode = effective_qdrant_mode(&plan);
        let limits = PlanLimits {
            plan_id: plan.id,
            plan_name: plan.name,
            rpm: plan.requests_per_minute,
            tpm: plan.tokens_per_minute,
            max_concurrent: plan.max_concurrent_requests,
            monthly_token_quota: plan.monthly_token_quota,
            billing_mode: plan.billing_mode,
            storage_bytes_quota: plan.storage_bytes_quota,
            max_file_size_bytes: plan.max_file_size_bytes,
            max_files: plan.max_files,
            embeddings_enabled: plan.embeddings_enabled,
            rag_enabled: plan.rag_enabled,
            qdrant_mode,
            price_amount: plan.price_amount,
            currency: plan.currency,
            trial_days: plan.trial_days,
            max_api_keys: plan.max_api_keys,
            overage_enabled: plan.overage_enabled,
            fair_use_policy: plan.fair_use_policy,
            grace_period_days: plan.grace_period_days,
            sso_enabled: plan.sso_enabled,
            sla_enabled: plan.sla_enabled,
            mtls_enabled: plan.mtls_enabled,
            s3_api_enabled: plan.s3_api_enabled,
            max_ingestion_files_per_hour: plan.max_ingestion_files_per_hour,
            max_ingestion_files_per_day: plan.max_ingestion_files_per_day,
            max_ingestion_bytes_per_day: plan.max_ingestion_bytes_per_day,
            max_ingestion_tokens_per_month: plan.max_ingestion_tokens_per_month,
            max_concurrent_ingestions: plan.max_concurrent_ingestions,
            max_pending_ingestion_jobs: plan.max_pending_ingestion_jobs,
        };
        self.cache.set(user_id, &limits).await?;
        Ok(Some(limits))
    }
}
pub struct ListActivePlans {
    pub plans: Arc<dyn PlanRepository>,
}
impl ListActivePlans {
    pub async fn execute(&self) -> Result<Vec<Plan>, SubscriptionError> {
        self.plans.find_all_active().await
    }
}
pub struct GetPlan {
    pub plans: Arc<dyn PlanRepository>,
}

impl GetPlan {
    pub async fn execute(&self, id: Uuid) -> Result<Plan, SubscriptionError> {
        self.plans
            .find_by_id(id)
            .await?
            .ok_or(SubscriptionError::PlanNotFound)
    }
}
pub struct GetSubscription {
    pub subscriptions: Arc<dyn SubscriptionRepository>,
}
impl GetSubscription {
    pub async fn by_id(&self, id: Uuid) -> Result<Option<Subscription>, SubscriptionError> {
        self.subscriptions.find_by_id(id).await
    }
    pub async fn active_by_user(
        &self,
        user_id: Uuid,
    ) -> Result<Option<Subscription>, SubscriptionError> {
        self.subscriptions.find_active_by_user(user_id).await
    }
}

#[cfg(test)]
mod tests {
    use super::effective_qdrant_mode;
    use crate::domain::entities::Plan;
    use chrono::Utc;
    use rust_decimal::Decimal;
    use uuid::Uuid;

    fn plan(mode: &str, features: serde_json::Value) -> Plan {
        Plan {
            id: Uuid::now_v7(),
            name: "Pro Plus".into(),
            slug: "pro-plus".into(),
            requests_per_minute: 1,
            tokens_per_minute: 1,
            max_concurrent_requests: 1,
            monthly_token_quota: None,
            price_multiplier: Decimal::ONE,
            billing_mode: "postpaid".into(),
            features,
            is_active: true,
            created_at: Utc::now(),
            storage_bytes_quota: 0,
            max_file_size_bytes: 0,
            max_files: 0,
            embeddings_enabled: true,
            rag_enabled: true,
            qdrant_mode: mode.into(),
            price_amount: Decimal::ZERO,
            currency: "USD".into(),
            trial_days: 0,
            max_api_keys: 0,
            overage_enabled: false,
            fair_use_policy: "fair_use".into(),
            grace_period_days: 0,
            sso_enabled: false,
            sla_enabled: false,
            mtls_enabled: false,
            s3_api_enabled: true,
            max_ingestion_files_per_hour: 1,
            max_ingestion_files_per_day: 1,
            max_ingestion_bytes_per_day: 1,
            max_ingestion_tokens_per_month: 1,
            max_concurrent_ingestions: 1,
            max_pending_ingestion_jobs: 1,
        }
    }

    #[test]
    fn pro_plus_is_shared_without_contractual_addon() {
        assert_eq!(
            effective_qdrant_mode(&plan("dedicated", serde_json::json!({}))),
            "none"
        );
        assert_eq!(
            effective_qdrant_mode(&plan(
                "shared",
                serde_json::json!({"dedicated_qdrant_enabled": false})
            )),
            "shared"
        );
    }

    #[test]
    fn dedicated_mode_requires_explicit_contractual_capability() {
        assert_eq!(
            effective_qdrant_mode(&plan(
                "shared",
                serde_json::json!({"dedicated_qdrant_enabled": true})
            )),
            "dedicated"
        );
    }
}
