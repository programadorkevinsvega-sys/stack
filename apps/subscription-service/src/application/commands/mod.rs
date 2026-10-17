use crate::application::ports::AuditLogger;
use crate::domain::entities::{Plan, Subscription};
use crate::domain::errors::SubscriptionError;
use crate::domain::repositories::{PlanLimitsCache, PlanRepository, SubscriptionRepository};
use chrono::{Duration, Utc};
use common::AuditEntry;
use rust_decimal::Decimal;
use serde_json::json;
use std::sync::Arc;
use uuid::Uuid;
async fn invalidate_plan_cache(cache: &Arc<dyn PlanLimitsCache>, subscription: &Subscription) {
    if let Err(error) = cache.invalidate(subscription.user_id).await {
        tracing::warn!(
            %error,
            user_id = %subscription.user_id,
            "subscription persisted and event enqueued, but cache invalidation failed"
        );
    }
}
fn plan_ok(input: &PlanInput) -> Result<(), SubscriptionError> {
    let dedicated_qdrant_enabled = input
        .features
        .get("dedicated_qdrant_enabled")
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if input.name.trim().is_empty()
        || input.slug.trim().is_empty()
        || input.requests_per_minute <= 0
        || input.tokens_per_minute <= 0
        || input.max_concurrent_requests <= 0
        || input.price_multiplier < Decimal::ONE
        || !matches!(input.billing_mode.as_str(), "prepaid" | "postpaid")
    {
        return Err(SubscriptionError::Validation(
            "invalid plan configuration".into(),
        ));
    }
    if input.price_amount < Decimal::ZERO
        || input.trial_days < 0
        || input.max_api_keys < 0
        || input.grace_period_days < 0
        || input.storage_bytes_quota < 0
        || input.max_file_size_bytes < 0
        || input.max_files < 0
        || input.max_ingestion_files_per_hour < 0
        || input.max_ingestion_files_per_day < 0
        || input.max_ingestion_bytes_per_day < 0
        || input.max_ingestion_tokens_per_month < 0
        || input.max_concurrent_ingestions <= 0
        || input.max_pending_ingestion_jobs <= 0
        || !matches!(input.currency.as_str(), "USD" | "COP" | "EUR")
        || !matches!(input.qdrant_mode.as_str(), "none" | "shared" | "dedicated")
        || (input.qdrant_mode == "dedicated" && !dedicated_qdrant_enabled)
        || (input.s3_api_enabled && input.qdrant_mode == "none")
    {
        return Err(SubscriptionError::Validation(
            "invalid plan capabilities or billing configuration".into(),
        ));
    }
    Ok(())
}
pub struct CreatePlan {
    pub repo: Arc<dyn PlanRepository>,
    pub audit_logger: Arc<dyn AuditLogger>,
}
pub struct PlanInput {
    pub name: String,
    pub slug: String,
    pub requests_per_minute: i32,
    pub tokens_per_minute: i32,
    pub max_concurrent_requests: i32,
    pub monthly_token_quota: Option<i64>,
    pub price_multiplier: Decimal,
    pub billing_mode: String,
    pub features: serde_json::Value,
    pub storage_bytes_quota: i64,
    pub max_file_size_bytes: i64,
    pub max_files: i64,
    pub embeddings_enabled: bool,
    pub rag_enabled: bool,
    pub qdrant_mode: String,
    pub price_amount: Decimal,
    pub currency: String,
    pub trial_days: i32,
    pub max_api_keys: i64,
    pub overage_enabled: bool,
    pub fair_use_policy: String,
    pub grace_period_days: i32,
    pub sso_enabled: bool,
    pub sla_enabled: bool,
    pub mtls_enabled: bool,
    pub s3_api_enabled: bool,
    pub max_ingestion_files_per_hour: i64,
    pub max_ingestion_files_per_day: i64,
    pub max_ingestion_bytes_per_day: i64,
    pub max_ingestion_tokens_per_month: i64,
    pub max_concurrent_ingestions: i64,
    pub max_pending_ingestion_jobs: i64,
}
pub struct UpdatePlanInput {
    pub plan: PlanInput,
    pub is_active: bool,
}
impl CreatePlan {
    pub async fn execute(&self, input: PlanInput) -> Result<Plan, SubscriptionError> {
        plan_ok(&input)?;
        let plan = Plan {
            id: Uuid::now_v7(),
            name: input.name,
            slug: input.slug,
            requests_per_minute: input.requests_per_minute,
            tokens_per_minute: input.tokens_per_minute,
            max_concurrent_requests: input.max_concurrent_requests,
            monthly_token_quota: input.monthly_token_quota,
            price_multiplier: input.price_multiplier,
            billing_mode: input.billing_mode,
            features: input.features,
            is_active: true,
            created_at: Utc::now(),
            storage_bytes_quota: input.storage_bytes_quota,
            max_file_size_bytes: input.max_file_size_bytes,
            max_files: input.max_files,
            embeddings_enabled: input.embeddings_enabled,
            rag_enabled: input.rag_enabled,
            qdrant_mode: input.qdrant_mode,
            price_amount: input.price_amount,
            currency: input.currency,
            trial_days: input.trial_days,
            max_api_keys: input.max_api_keys,
            overage_enabled: input.overage_enabled,
            fair_use_policy: input.fair_use_policy,
            grace_period_days: input.grace_period_days,
            sso_enabled: input.sso_enabled,
            sla_enabled: input.sla_enabled,
            mtls_enabled: input.mtls_enabled,
            s3_api_enabled: input.s3_api_enabled,
            max_ingestion_files_per_hour: input.max_ingestion_files_per_hour,
            max_ingestion_files_per_day: input.max_ingestion_files_per_day,
            max_ingestion_bytes_per_day: input.max_ingestion_bytes_per_day,
            max_ingestion_tokens_per_month: input.max_ingestion_tokens_per_month,
            max_concurrent_ingestions: input.max_concurrent_ingestions,
            max_pending_ingestion_jobs: input.max_pending_ingestion_jobs,
        };
        self.repo.save(&plan).await?;

        if let Err(e) = self
            .audit_logger
            .record(AuditEntry {
                actor_type: "system".into(),
                actor_id: None,
                action: "plan.created".into(),
                resource_type: "plan".into(),
                resource_id: Some(plan.id),
                metadata: json!({"name": plan.name, "slug": plan.slug}),
                ip_address: None,
            })
            .await
        {
            tracing::warn!(error = %e, plan_id = %plan.id, "Failed to record audit log for plan creation");
        }

        Ok(plan)
    }
}
pub struct UpdatePlan {
    pub repo: Arc<dyn PlanRepository>,
    pub audit_logger: Arc<dyn AuditLogger>,
    pub cache: Arc<dyn PlanLimitsCache>,
}
impl UpdatePlan {
    pub async fn execute(
        &self,
        id: Uuid,
        input: UpdatePlanInput,
    ) -> Result<Plan, SubscriptionError> {
        plan_ok(&input.plan)?;
        let mut plan = self
            .repo
            .find_by_id(id)
            .await?
            .ok_or(SubscriptionError::PlanNotFound)?;
        plan.name = input.plan.name;
        plan.slug = input.plan.slug;
        plan.requests_per_minute = input.plan.requests_per_minute;
        plan.tokens_per_minute = input.plan.tokens_per_minute;
        plan.max_concurrent_requests = input.plan.max_concurrent_requests;
        plan.monthly_token_quota = input.plan.monthly_token_quota;
        plan.price_multiplier = input.plan.price_multiplier;
        plan.billing_mode = input.plan.billing_mode;
        plan.features = input.plan.features;
        plan.storage_bytes_quota = input.plan.storage_bytes_quota;
        plan.max_file_size_bytes = input.plan.max_file_size_bytes;
        plan.max_files = input.plan.max_files;
        plan.embeddings_enabled = input.plan.embeddings_enabled;
        plan.rag_enabled = input.plan.rag_enabled;
        plan.qdrant_mode = input.plan.qdrant_mode;
        plan.price_amount = input.plan.price_amount;
        plan.currency = input.plan.currency;
        plan.trial_days = input.plan.trial_days;
        plan.max_api_keys = input.plan.max_api_keys;
        plan.overage_enabled = input.plan.overage_enabled;
        plan.fair_use_policy = input.plan.fair_use_policy;
        plan.grace_period_days = input.plan.grace_period_days;
        plan.sso_enabled = input.plan.sso_enabled;
        plan.sla_enabled = input.plan.sla_enabled;
        plan.mtls_enabled = input.plan.mtls_enabled;
        plan.s3_api_enabled = input.plan.s3_api_enabled;
        plan.max_ingestion_files_per_hour = input.plan.max_ingestion_files_per_hour;
        plan.max_ingestion_files_per_day = input.plan.max_ingestion_files_per_day;
        plan.max_ingestion_bytes_per_day = input.plan.max_ingestion_bytes_per_day;
        plan.max_ingestion_tokens_per_month = input.plan.max_ingestion_tokens_per_month;
        plan.max_concurrent_ingestions = input.plan.max_concurrent_ingestions;
        plan.max_pending_ingestion_jobs = input.plan.max_pending_ingestion_jobs;
        plan.is_active = input.is_active;
        self.repo.update(&plan).await?;
        self.cache.invalidate_plan(id).await?;

        if let Err(e) = self
            .audit_logger
            .record(AuditEntry {
                actor_type: "system".into(),
                actor_id: None,
                action: "plan.updated".into(),
                resource_type: "plan".into(),
                resource_id: Some(id),
                metadata: json!({"name": plan.name, "slug": plan.slug, "is_active": plan.is_active}),
                ip_address: None,
            })
            .await
        {
            tracing::warn!(error = %e, plan_id = %id, "Failed to record audit log for plan update");
        }

        Ok(plan)
    }
}
pub struct DeactivatePlan {
    pub repo: Arc<dyn PlanRepository>,
    pub audit_logger: Arc<dyn AuditLogger>,
    pub cache: Arc<dyn PlanLimitsCache>,
}
impl DeactivatePlan {
    pub async fn execute(&self, id: Uuid) -> Result<Plan, SubscriptionError> {
        let mut plan = self
            .repo
            .find_by_id(id)
            .await?
            .ok_or(SubscriptionError::PlanNotFound)?;
        plan.is_active = false;
        self.repo.update(&plan).await?;
        self.cache.invalidate_plan(id).await?;

        if let Err(e) = self
            .audit_logger
            .record(AuditEntry {
                actor_type: "system".into(),
                actor_id: None,
                action: "plan.deactivated".into(),
                resource_type: "plan".into(),
                resource_id: Some(id),
                metadata: json!({"name": plan.name, "slug": plan.slug}),
                ip_address: None,
            })
            .await
        {
            tracing::warn!(error = %e, plan_id = %id, "Failed to record audit log for plan deactivation");
        }

        Ok(plan)
    }
}
pub struct CreateSubscription {
    pub repo: Arc<dyn SubscriptionRepository>,
    pub plans: Arc<dyn PlanRepository>,
    pub cache: Arc<dyn PlanLimitsCache>,
    pub audit_logger: Arc<dyn AuditLogger>,
}
impl CreateSubscription {
    pub async fn execute(
        &self,
        user_id: Uuid,
        plan_id: Uuid,
        billing_cycle: String,
    ) -> Result<Subscription, SubscriptionError> {
        if !matches!(billing_cycle.as_str(), "monthly" | "annual") {
            return Err(SubscriptionError::Validation(
                "invalid billing cycle".into(),
            ));
        }
        if self.repo.find_current_by_user(user_id).await?.is_some() {
            return Err(SubscriptionError::Validation(
                "user already has a current subscription".into(),
            ));
        }
        let plan = self
            .plans
            .find_by_id(plan_id)
            .await?
            .filter(|v| v.is_active)
            .ok_or(SubscriptionError::PlanNotFound)?;
        let now = Utc::now();
        let period = if plan.trial_days > 0 {
            Duration::days(plan.trial_days as i64)
        } else if billing_cycle == "annual" {
            Duration::days(365)
        } else {
            Duration::days(30)
        };
        let value = Subscription {
            id: Uuid::now_v7(),
            user_id,
            plan_id: plan.id,
            status: if plan.trial_days > 0 {
                "trialing"
            } else {
                "active"
            }
            .into(),
            billing_cycle,
            current_period_start: now,
            current_period_end: now + period,
            cancel_at_period_end: false,
            canceled_at: None,
            created_at: now,
            updated_at: now,
            trial_ends_at: (plan.trial_days > 0)
                .then_some(now + Duration::days(plan.trial_days as i64)),
            grace_period_ends_at: None,
            payment_retry_count: 0,
            last_payment_error: None,
            provider_subscription_id: None,
        };
        self.repo.save(&value).await?;
        invalidate_plan_cache(&self.cache, &value).await;

        if let Err(e) = self
            .audit_logger
            .record(AuditEntry {
                actor_type: "system".into(),
                actor_id: None,
                action: "subscription.created".into(),
                resource_type: "subscription".into(),
                resource_id: Some(value.id),
                metadata: json!({"user_id": value.user_id.to_string(), "plan_id": plan.id.to_string(), "plan_name": plan.name}),
                ip_address: None,
            })
            .await
        {
            tracing::warn!(error = %e, subscription_id = %value.id, "Failed to record audit log for subscription creation");
        }

        Ok(value)
    }
}
pub struct ChangeSubscription {
    pub repo: Arc<dyn SubscriptionRepository>,
    pub plans: Arc<dyn PlanRepository>,
    pub cache: Arc<dyn PlanLimitsCache>,
    pub audit_logger: Arc<dyn AuditLogger>,
}
impl ChangeSubscription {
    pub async fn change_plan(
        &self,
        user_id: Uuid,
        plan_id: Uuid,
    ) -> Result<Subscription, SubscriptionError> {
        self.plans
            .find_by_id(plan_id)
            .await?
            .filter(|v| v.is_active)
            .ok_or(SubscriptionError::PlanNotFound)?;
        let mut s = self
            .repo
            .find_current_by_user(user_id)
            .await?
            .ok_or(SubscriptionError::SubscriptionNotFound)?;
        if s.plan_id == plan_id {
            return Err(SubscriptionError::Validation(
                "new plan is the same as current plan".into(),
            ));
        }
        s.plan_id = plan_id;
        s.updated_at = Utc::now();
        self.repo.update(&s).await?;
        invalidate_plan_cache(&self.cache, &s).await;

        if let Err(e) = self
            .audit_logger
            .record(AuditEntry {
                actor_type: "system".into(),
                actor_id: None,
                action: "subscription.plan_changed".into(),
                resource_type: "subscription".into(),
                resource_id: Some(s.id),
                metadata: json!({"user_id": user_id.to_string(), "new_plan_id": plan_id.to_string()}),
                ip_address: None,
            })
            .await
        {
            tracing::warn!(error = %e, subscription_id = %s.id, "Failed to record audit log for plan change");
        }

        Ok(s)
    }
    pub async fn cancel(
        &self,
        user_id: Uuid,
        at_period_end: bool,
    ) -> Result<Subscription, SubscriptionError> {
        let mut s = self
            .repo
            .find_active_by_user(user_id)
            .await?
            .ok_or(SubscriptionError::SubscriptionNotFound)?;
        s.cancel_at_period_end = at_period_end;
        if !at_period_end {
            s.status = "canceled".into();
            s.canceled_at = Some(Utc::now())
        }
        s.updated_at = Utc::now();
        self.repo.update(&s).await?;
        invalidate_plan_cache(&self.cache, &s).await;

        if let Err(e) = self
            .audit_logger
            .record(AuditEntry {
                actor_type: "system".into(),
                actor_id: None,
                action: "subscription.canceled".into(),
                resource_type: "subscription".into(),
                resource_id: Some(s.id),
                metadata: json!({"user_id": user_id.to_string(), "at_period_end": at_period_end}),
                ip_address: None,
            })
            .await
        {
            tracing::warn!(error = %e, subscription_id = %s.id, "Failed to record audit log for subscription cancellation");
        }

        Ok(s)
    }
    pub async fn renew(&self, user_id: Uuid) -> Result<Subscription, SubscriptionError> {
        let mut s = self
            .repo
            .find_current_by_user(user_id)
            .await?
            .ok_or(SubscriptionError::SubscriptionNotFound)?;
        let period = if s.billing_cycle == "annual" {
            Duration::days(365)
        } else {
            Duration::days(30)
        };
        let now = Utc::now();
        s.current_period_start = now;
        s.current_period_end = now + period;
        s.status = "active".into();
        s.cancel_at_period_end = false;
        if s.canceled_at.is_some() {
            s.canceled_at = None;
        }
        s.updated_at = Utc::now();
        self.repo.update(&s).await?;
        invalidate_plan_cache(&self.cache, &s).await;

        if let Err(e) = self
            .audit_logger
            .record(AuditEntry {
                actor_type: "system".into(),
                actor_id: None,
                action: "subscription.renewed".into(),
                resource_type: "subscription".into(),
                resource_id: Some(s.id),
                metadata: json!({"user_id": user_id.to_string()}),
                ip_address: None,
            })
            .await
        {
            tracing::warn!(error = %e, subscription_id = %s.id, "Failed to record audit log for subscription renewal");
        }

        Ok(s)
    }
    pub async fn past_due(&self, id: Uuid) -> Result<Subscription, SubscriptionError> {
        let mut s = self
            .repo
            .find_by_id(id)
            .await?
            .ok_or(SubscriptionError::SubscriptionNotFound)?;
        s.status = "past_due".into();
        s.payment_retry_count += 1;
        s.last_payment_error = Some("payment_failed".into());
        s.grace_period_ends_at = Some(Utc::now() + Duration::days(3));
        s.updated_at = Utc::now();
        self.repo.update(&s).await?;
        invalidate_plan_cache(&self.cache, &s).await;
        Ok(s)
    }
}
