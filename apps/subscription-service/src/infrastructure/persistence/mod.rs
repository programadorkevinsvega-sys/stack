use crate::application::ports::AuditLogger;
use crate::domain::entities::{Plan, Subscription};
use crate::domain::errors::SubscriptionError;
use crate::domain::repositories::{PlanRepository, SubscriptionRepository};
use async_trait::async_trait;
use common::{AuditEntry, PgAuditLogger};
use events::{
    outbox::enqueue_in_transaction, EventEnvelope, SubscriptionChanged, EVENT_SUBSCRIPTION_CHANGED,
};
use sqlx::PgPool;
use uuid::Uuid;

pub struct PgAuditLoggerAdapter {
    inner: PgAuditLogger,
}

impl PgAuditLoggerAdapter {
    pub fn new(inner: PgAuditLogger) -> Self {
        Self { inner }
    }
}

#[async_trait]
impl AuditLogger for PgAuditLoggerAdapter {
    async fn record(&self, entry: AuditEntry) -> Result<(), SubscriptionError> {
        self.inner
            .record(entry)
            .await
            .map_err(SubscriptionError::DependencyUnavailable)
    }
}
pub struct PgPlanRepository {
    pub pool: PgPool,
}
pub struct PgSubscriptionRepository {
    pub pool: PgPool,
}
#[async_trait]
impl PlanRepository for PgPlanRepository {
    async fn save(&self, plan: &Plan) -> Result<(), SubscriptionError> {
        sqlx::query("INSERT INTO plans (id,name,slug,requests_per_minute,tokens_per_minute,max_concurrent_requests,monthly_token_quota,price_multiplier,billing_mode,features,is_active,created_at,storage_bytes_quota,max_file_size_bytes,max_files,embeddings_enabled,rag_enabled,qdrant_mode,price_amount,currency,trial_days,max_api_keys,overage_enabled,fair_use_policy,grace_period_days,sso_enabled,sla_enabled,mtls_enabled,s3_api_enabled,max_ingestion_files_per_hour,max_ingestion_files_per_day,max_ingestion_bytes_per_day,max_ingestion_tokens_per_month,max_concurrent_ingestions,max_pending_ingestion_jobs) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,$25,$26,$27,$28,$29,$30,$31,$32,$33,$34,$35)")
            .bind(plan.id).bind(&plan.name).bind(&plan.slug).bind(plan.requests_per_minute).bind(plan.tokens_per_minute)
            .bind(plan.max_concurrent_requests).bind(plan.monthly_token_quota).bind(plan.price_multiplier)
            .bind(&plan.billing_mode).bind(&plan.features).bind(plan.is_active).bind(plan.created_at)
            .bind(plan.storage_bytes_quota).bind(plan.max_file_size_bytes).bind(plan.max_files)
            .bind(plan.embeddings_enabled).bind(plan.rag_enabled).bind(&plan.qdrant_mode)
            .bind(plan.price_amount).bind(&plan.currency).bind(plan.trial_days).bind(plan.max_api_keys)
            .bind(plan.overage_enabled).bind(&plan.fair_use_policy).bind(plan.grace_period_days)
            .bind(plan.sso_enabled).bind(plan.sla_enabled).bind(plan.mtls_enabled).bind(plan.s3_api_enabled)
            .bind(plan.max_ingestion_files_per_hour).bind(plan.max_ingestion_files_per_day)
            .bind(plan.max_ingestion_bytes_per_day).bind(plan.max_ingestion_tokens_per_month)
            .bind(plan.max_concurrent_ingestions).bind(plan.max_pending_ingestion_jobs)
            .execute(&self.pool).await.map_err(|e|SubscriptionError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Plan>, SubscriptionError> {
        sqlx::query_as("SELECT * FROM plans WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))
    }
    async fn find_all_active(&self) -> Result<Vec<Plan>, SubscriptionError> {
        sqlx::query_as("SELECT * FROM plans WHERE is_active=true ORDER BY price_multiplier")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))
    }
    async fn update(&self, plan: &Plan) -> Result<(), SubscriptionError> {
        sqlx::query("UPDATE plans SET name=$2,slug=$3,requests_per_minute=$4,tokens_per_minute=$5,max_concurrent_requests=$6,monthly_token_quota=$7,price_multiplier=$8,billing_mode=$9,features=$10,is_active=$11,storage_bytes_quota=$12,max_file_size_bytes=$13,max_files=$14,embeddings_enabled=$15,rag_enabled=$16,qdrant_mode=$17,price_amount=$18,currency=$19,trial_days=$20,max_api_keys=$21,overage_enabled=$22,fair_use_policy=$23,grace_period_days=$24,sso_enabled=$25,sla_enabled=$26,mtls_enabled=$27,s3_api_enabled=$28,max_ingestion_files_per_hour=$29,max_ingestion_files_per_day=$30,max_ingestion_bytes_per_day=$31,max_ingestion_tokens_per_month=$32,max_concurrent_ingestions=$33,max_pending_ingestion_jobs=$34 WHERE id=$1")
            .bind(plan.id).bind(&plan.name).bind(&plan.slug).bind(plan.requests_per_minute).bind(plan.tokens_per_minute)
            .bind(plan.max_concurrent_requests).bind(plan.monthly_token_quota).bind(plan.price_multiplier)
            .bind(&plan.billing_mode).bind(&plan.features).bind(plan.is_active).bind(plan.storage_bytes_quota)
            .bind(plan.max_file_size_bytes).bind(plan.max_files).bind(plan.embeddings_enabled).bind(plan.rag_enabled)
            .bind(&plan.qdrant_mode).bind(plan.price_amount).bind(&plan.currency).bind(plan.trial_days)
            .bind(plan.max_api_keys).bind(plan.overage_enabled).bind(&plan.fair_use_policy)
            .bind(plan.grace_period_days).bind(plan.sso_enabled).bind(plan.sla_enabled)
            .bind(plan.mtls_enabled).bind(plan.s3_api_enabled)
            .bind(plan.max_ingestion_files_per_hour).bind(plan.max_ingestion_files_per_day)
            .bind(plan.max_ingestion_bytes_per_day).bind(plan.max_ingestion_tokens_per_month)
            .bind(plan.max_concurrent_ingestions).bind(plan.max_pending_ingestion_jobs)
            .execute(&self.pool).await.map_err(|e|SubscriptionError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }
    async fn accept_terms(
        &self,
        user_id: Uuid,
        terms_version: &str,
        privacy_version: &str,
        ip_address: Option<&str>,
        user_agent: &str,
        country: Option<&str>,
    ) -> Result<chrono::DateTime<chrono::Utc>, SubscriptionError> {
        let value = sqlx::query_scalar::<_, chrono::DateTime<chrono::Utc>>("INSERT INTO terms_acceptances (id,user_id,terms_version,privacy_version,ip_address,user_agent,country) VALUES ($1,$2,$3,$4,$5::inet,$6,$7) ON CONFLICT (user_id,terms_version,privacy_version) DO UPDATE SET accepted_at=terms_acceptances.accepted_at RETURNING accepted_at")
            .bind(Uuid::now_v7()).bind(user_id).bind(terms_version).bind(privacy_version).bind(ip_address).bind(user_agent).bind(country)
            .fetch_one(&self.pool).await.map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
        Ok(value)
    }
    async fn has_terms_acceptance(
        &self,
        user_id: Uuid,
        terms_version: &str,
        privacy_version: &str,
    ) -> Result<bool, SubscriptionError> {
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM terms_acceptances WHERE user_id=$1 AND terms_version=$2 AND privacy_version=$3)").bind(user_id).bind(terms_version).bind(privacy_version).fetch_one(&self.pool).await.map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))
    }
}
#[async_trait]
impl SubscriptionRepository for PgSubscriptionRepository {
    async fn save(&self, value: &Subscription) -> Result<(), SubscriptionError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1::text))")
            .bind(value.user_id)
            .execute(tx.as_mut())
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
        if value.status == "active" || value.status == "trialing" {
            let conflict = sqlx::query_scalar!(
                "SELECT EXISTS(SELECT 1 FROM subscriptions WHERE user_id=$1 AND status IN ('trialing','active') AND id != $2) AS \"exists!\"",
                value.user_id,
                value.id,
            )
            .fetch_one(tx.as_mut())
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
            if conflict {
                return Err(SubscriptionError::Validation(
                    "user already has an active subscription".into(),
                ));
            }
        }
        sqlx::query("INSERT INTO subscriptions (id,user_id,plan_id,status,billing_cycle,current_period_start,current_period_end,cancel_at_period_end,canceled_at,created_at,updated_at,trial_ends_at,grace_period_ends_at,payment_retry_count,last_payment_error,provider_subscription_id) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)").bind(value.id).bind(value.user_id).bind(value.plan_id).bind(&value.status).bind(&value.billing_cycle).bind(value.current_period_start).bind(value.current_period_end).bind(value.cancel_at_period_end).bind(value.canceled_at).bind(value.created_at).bind(value.updated_at).bind(value.trial_ends_at).bind(value.grace_period_ends_at).bind(value.payment_retry_count).bind(&value.last_payment_error).bind(&value.provider_subscription_id).execute(tx.as_mut()).await.map_err(|e|SubscriptionError::DependencyUnavailable(e.to_string()))?;
        let envelope = EventEnvelope::new(
            EVENT_SUBSCRIPTION_CHANGED,
            SubscriptionChanged {
                user_id: value.user_id,
                old_plan_id: value.plan_id,
                new_plan_id: value.plan_id,
                changed_at: value.updated_at,
            },
        );
        enqueue_in_transaction(
            &mut tx,
            "event_outbox",
            EVENT_SUBSCRIPTION_CHANGED,
            &envelope,
        )
        .await
        .map_err(|error| SubscriptionError::DependencyUnavailable(error.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }
    async fn find_by_id(&self, id: Uuid) -> Result<Option<Subscription>, SubscriptionError> {
        sqlx::query_as("SELECT * FROM subscriptions WHERE id=$1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))
    }
    async fn find_active_by_user(
        &self,
        user_id: Uuid,
    ) -> Result<Option<Subscription>, SubscriptionError> {
        sqlx::query_as("SELECT * FROM subscriptions WHERE user_id=$1 AND status IN ('trialing','active') ORDER BY created_at DESC LIMIT 1").bind(user_id).fetch_optional(&self.pool).await.map_err(|e|SubscriptionError::DependencyUnavailable(e.to_string()))
    }
    async fn find_current_by_user(
        &self,
        user_id: Uuid,
    ) -> Result<Option<Subscription>, SubscriptionError> {
        sqlx::query_as("SELECT * FROM subscriptions WHERE user_id=$1 AND status IN ('trialing','active','past_due') ORDER BY created_at DESC LIMIT 1").bind(user_id).fetch_optional(&self.pool).await.map_err(|e|SubscriptionError::DependencyUnavailable(e.to_string()))
    }
    async fn update(&self, value: &Subscription) -> Result<(), SubscriptionError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
        let old_plan_id = sqlx::query_scalar::<_, Uuid>(
            "SELECT plan_id FROM subscriptions WHERE id=$1 FOR UPDATE",
        )
        .bind(value.id)
        .fetch_optional(tx.as_mut())
        .await
        .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?
        .ok_or(SubscriptionError::SubscriptionNotFound)?;
        sqlx::query("UPDATE subscriptions SET plan_id=$2,status=$3,billing_cycle=$4,current_period_start=$5,current_period_end=$6,cancel_at_period_end=$7,canceled_at=$8,updated_at=$9,trial_ends_at=$10,grace_period_ends_at=$11,payment_retry_count=$12,last_payment_error=$13,provider_subscription_id=$14 WHERE id=$1").bind(value.id).bind(value.plan_id).bind(&value.status).bind(&value.billing_cycle).bind(value.current_period_start).bind(value.current_period_end).bind(value.cancel_at_period_end).bind(value.canceled_at).bind(value.updated_at).bind(value.trial_ends_at).bind(value.grace_period_ends_at).bind(value.payment_retry_count).bind(&value.last_payment_error).bind(&value.provider_subscription_id).execute(tx.as_mut()).await.map_err(|e|SubscriptionError::DependencyUnavailable(e.to_string()))?;
        let envelope = EventEnvelope::new(
            EVENT_SUBSCRIPTION_CHANGED,
            SubscriptionChanged {
                user_id: value.user_id,
                old_plan_id,
                new_plan_id: value.plan_id,
                changed_at: value.updated_at,
            },
        );
        enqueue_in_transaction(
            &mut tx,
            "event_outbox",
            EVENT_SUBSCRIPTION_CHANGED,
            &envelope,
        )
        .await
        .map_err(|error| SubscriptionError::DependencyUnavailable(error.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| SubscriptionError::DependencyUnavailable(e.to_string()))?;
        Ok(())
    }
}
