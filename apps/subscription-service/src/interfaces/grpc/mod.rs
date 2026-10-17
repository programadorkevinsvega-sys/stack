use crate::application::commands::{
    ChangeSubscription, CreatePlan, CreateSubscription, DeactivatePlan, PlanInput, UpdatePlan,
    UpdatePlanInput,
};
use crate::application::queries::{GetPlan, GetPlanLimits, GetSubscription, ListActivePlans};
use crate::domain::entities::{Plan as DomainPlan, PlanLimits, Subscription as DomainSubscription};
use crate::domain::errors::SubscriptionError;
use common::ServiceJwtClaims;
use proto::plan_resolution_v1::plan_resolution_service_server::PlanResolutionService;
use proto::plan_resolution_v1::subscription_management_service_server::SubscriptionManagementService;
use proto::plan_resolution_v1::{
    AcceptTermsRequest, AcceptTermsResponse, CancelSubscriptionRequest,
    ChangeSubscriptionPlanRequest, CreatePlanRequest, CreateSubscriptionRequest,
    DeactivatePlanRequest, GetActiveSubscriptionRequest, GetPlanLimitsRequest,
    GetPlanLimitsResponse, GetPlanRequest, GetSubscriptionRequest, GetSubscriptionResponse,
    GetTermsStatusRequest, GetTermsStatusResponse, ListActivePlansRequest, ListPlansResponse,
    MarkSubscriptionPastDueRequest, Plan, PlanResponse, RenewSubscriptionRequest, Subscription,
    SubscriptionResponse, UpdatePlanRequest,
};
use rust_decimal::Decimal;
use serde_json::Value;
use std::collections::HashSet;
use tonic::{Request, Response, Status};
use uuid::Uuid;

pub struct PlanGrpcService {
    pub limits: GetPlanLimits,
}

pub struct SubscriptionManagementGrpcService {
    pub create_plan: CreatePlan,
    pub update_plan: UpdatePlan,
    pub deactivate_plan: DeactivatePlan,
    pub list_plans: ListActivePlans,
    pub get_plan: GetPlan,
    pub create_subscription: CreateSubscription,
    pub change_subscription: ChangeSubscription,
    pub get_subscription: GetSubscription,
    pub admin_callers: HashSet<String>,
}

fn require_admin<T>(request: &Request<T>, admin_callers: &HashSet<String>) -> Result<(), Status> {
    let claims = request
        .extensions()
        .get::<ServiceJwtClaims>()
        .ok_or_else(|| Status::unauthenticated("missing service claims"))?;
    if admin_callers.contains(&claims.sub) {
        Ok(())
    } else {
        Err(Status::permission_denied(
            "caller is not allowed for admin operations",
        ))
    }
}

fn uuid(raw: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw).map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
}

fn decimal(raw: &str, field: &str) -> Result<Decimal, Status> {
    raw.parse::<Decimal>()
        .map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
}

fn json(raw: &str) -> Result<Value, Status> {
    if raw.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(raw)
        .map_err(|e| Status::invalid_argument(format!("invalid features_json: {e}")))
}

fn map_error(error: SubscriptionError) -> Status {
    match error {
        SubscriptionError::PlanNotFound => Status::not_found("plan not found"),
        SubscriptionError::SubscriptionNotFound => Status::not_found("subscription not found"),
        SubscriptionError::SubscriptionNotActive => {
            Status::failed_precondition("subscription not active")
        }
        SubscriptionError::Validation(message) => Status::invalid_argument(message),
        SubscriptionError::DependencyUnavailable(message) => Status::unavailable(message),
    }
}

fn plan(value: DomainPlan) -> Plan {
    Plan {
        id: value.id.to_string(),
        name: value.name,
        slug: value.slug,
        requests_per_minute: value.requests_per_minute,
        tokens_per_minute: value.tokens_per_minute,
        max_concurrent_requests: value.max_concurrent_requests,
        monthly_token_quota: value.monthly_token_quota,
        price_multiplier: value.price_multiplier.to_string(),
        billing_mode: value.billing_mode,
        features_json: value.features.to_string(),
        is_active: value.is_active,
        created_at: value.created_at.to_rfc3339(),
        storage_bytes_quota: value.storage_bytes_quota,
        max_file_size_bytes: value.max_file_size_bytes,
        max_files: value.max_files,
        embeddings_enabled: value.embeddings_enabled,
        rag_enabled: value.rag_enabled,
        qdrant_mode: value.qdrant_mode,
        price_amount: value.price_amount.to_string(),
        currency: value.currency,
        trial_days: value.trial_days,
        max_api_keys: value.max_api_keys,
        overage_enabled: value.overage_enabled,
        fair_use_policy: value.fair_use_policy,
        grace_period_days: value.grace_period_days,
        sso_enabled: value.sso_enabled,
        sla_enabled: value.sla_enabled,
        mtls_enabled: value.mtls_enabled,
        s3_api_enabled: value.s3_api_enabled,
        max_ingestion_files_per_hour: value.max_ingestion_files_per_hour,
        max_ingestion_files_per_day: value.max_ingestion_files_per_day,
        max_ingestion_bytes_per_day: value.max_ingestion_bytes_per_day,
        max_ingestion_tokens_per_month: value.max_ingestion_tokens_per_month,
        max_concurrent_ingestions: value.max_concurrent_ingestions,
        max_pending_ingestion_jobs: value.max_pending_ingestion_jobs,
    }
}

fn subscription(value: DomainSubscription) -> Subscription {
    Subscription {
        id: value.id.to_string(),
        user_id: value.user_id.to_string(),
        plan_id: value.plan_id.to_string(),
        status: value.status,
        billing_cycle: value.billing_cycle,
        current_period_start: value.current_period_start.to_rfc3339(),
        current_period_end: value.current_period_end.to_rfc3339(),
        cancel_at_period_end: value.cancel_at_period_end,
        canceled_at: value.canceled_at.map(|v| v.to_rfc3339()),
        created_at: value.created_at.to_rfc3339(),
        updated_at: value.updated_at.to_rfc3339(),
        trial_ends_at: value.trial_ends_at.map(|v| v.to_rfc3339()),
        grace_period_ends_at: value.grace_period_ends_at.map(|v| v.to_rfc3339()),
        payment_retry_count: value.payment_retry_count,
        last_payment_error: value.last_payment_error,
        provider_subscription_id: value.provider_subscription_id,
    }
}

fn limits_response(value: Option<PlanLimits>) -> GetPlanLimitsResponse {
    match value {
        Some(v) => GetPlanLimitsResponse {
            has_active_subscription: true,
            requests_per_minute: v.rpm,
            tokens_per_minute: v.tpm,
            max_concurrent_requests: v.max_concurrent,
            monthly_token_quota: v.monthly_token_quota,
            billing_mode: v.billing_mode,
            plan_id: v.plan_id.to_string(),
            plan_name: v.plan_name,
            storage_bytes_quota: v.storage_bytes_quota,
            max_file_size_bytes: v.max_file_size_bytes,
            max_files: v.max_files,
            embeddings_enabled: v.embeddings_enabled,
            rag_enabled: v.rag_enabled,
            qdrant_mode: v.qdrant_mode,
            price_amount: v.price_amount.to_string(),
            currency: v.currency,
            trial_days: v.trial_days,
            max_api_keys: v.max_api_keys,
            overage_enabled: v.overage_enabled,
            fair_use_policy: v.fair_use_policy,
            grace_period_days: v.grace_period_days,
            sso_enabled: v.sso_enabled,
            sla_enabled: v.sla_enabled,
            mtls_enabled: v.mtls_enabled,
            s3_api_enabled: v.s3_api_enabled,
            max_ingestion_files_per_hour: v.max_ingestion_files_per_hour,
            max_ingestion_files_per_day: v.max_ingestion_files_per_day,
            max_ingestion_bytes_per_day: v.max_ingestion_bytes_per_day,
            max_ingestion_tokens_per_month: v.max_ingestion_tokens_per_month,
            max_concurrent_ingestions: v.max_concurrent_ingestions,
            max_pending_ingestion_jobs: v.max_pending_ingestion_jobs,
        },
        None => GetPlanLimitsResponse {
            has_active_subscription: false,
            requests_per_minute: 0,
            tokens_per_minute: 0,
            max_concurrent_requests: 0,
            monthly_token_quota: None,
            billing_mode: String::new(),
            plan_id: String::new(),
            plan_name: String::new(),
            storage_bytes_quota: 0,
            max_file_size_bytes: 0,
            max_files: 0,
            embeddings_enabled: false,
            rag_enabled: false,
            qdrant_mode: String::new(),
            currency: String::new(),
            price_amount: String::new(),
            trial_days: 0,
            max_api_keys: 0,
            overage_enabled: false,
            fair_use_policy: String::new(),
            grace_period_days: 0,
            sso_enabled: false,
            sla_enabled: false,
            mtls_enabled: false,
            s3_api_enabled: false,
            max_ingestion_files_per_hour: 0,
            max_ingestion_files_per_day: 0,
            max_ingestion_bytes_per_day: 0,
            max_ingestion_tokens_per_month: 0,
            max_concurrent_ingestions: 0,
            max_pending_ingestion_jobs: 0,
        },
    }
}

#[tonic::async_trait]
impl PlanResolutionService for PlanGrpcService {
    async fn get_plan_limits(
        &self,
        request: Request<GetPlanLimitsRequest>,
    ) -> Result<Response<GetPlanLimitsResponse>, Status> {
        let user_id = uuid(&request.into_inner().user_id, "user_id")?;
        let limits = self.limits.execute(user_id).await.map_err(map_error)?;
        Ok(Response::new(limits_response(limits)))
    }
}

#[tonic::async_trait]
impl SubscriptionManagementService for SubscriptionManagementGrpcService {
    async fn create_plan(
        &self,
        request: Request<CreatePlanRequest>,
    ) -> Result<Response<PlanResponse>, Status> {
        require_admin(&request, &self.admin_callers)?;
        let r = request.into_inner();
        let value = self
            .create_plan
            .execute(PlanInput {
                name: r.name,
                slug: r.slug,
                requests_per_minute: r.requests_per_minute,
                tokens_per_minute: r.tokens_per_minute,
                max_concurrent_requests: r.max_concurrent_requests,
                monthly_token_quota: r.monthly_token_quota,
                price_multiplier: decimal(&r.price_multiplier, "price_multiplier")?,
                billing_mode: r.billing_mode,
                features: json(&r.features_json)?,
                storage_bytes_quota: r.storage_bytes_quota,
                max_file_size_bytes: r.max_file_size_bytes,
                max_files: r.max_files,
                embeddings_enabled: r.embeddings_enabled,
                rag_enabled: r.rag_enabled,
                qdrant_mode: r.qdrant_mode,
                price_amount: decimal(&r.price_amount, "price_amount")?,
                currency: r.currency,
                trial_days: r.trial_days,
                max_api_keys: r.max_api_keys,
                overage_enabled: r.overage_enabled,
                fair_use_policy: r.fair_use_policy,
                grace_period_days: r.grace_period_days,
                sso_enabled: r.sso_enabled,
                sla_enabled: r.sla_enabled,
                mtls_enabled: r.mtls_enabled,
                s3_api_enabled: r.s3_api_enabled,
                max_ingestion_files_per_hour: r.max_ingestion_files_per_hour,
                max_ingestion_files_per_day: r.max_ingestion_files_per_day,
                max_ingestion_bytes_per_day: r.max_ingestion_bytes_per_day,
                max_ingestion_tokens_per_month: r.max_ingestion_tokens_per_month,
                max_concurrent_ingestions: r.max_concurrent_ingestions,
                max_pending_ingestion_jobs: r.max_pending_ingestion_jobs,
            })
            .await
            .map_err(map_error)?;
        Ok(Response::new(PlanResponse {
            plan: Some(plan(value)),
        }))
    }

    async fn update_plan(
        &self,
        request: Request<UpdatePlanRequest>,
    ) -> Result<Response<PlanResponse>, Status> {
        require_admin(&request, &self.admin_callers)?;
        let r = request.into_inner();
        let value = self
            .update_plan
            .execute(
                uuid(&r.plan_id, "plan_id")?,
                UpdatePlanInput {
                    plan: PlanInput {
                        name: r.name,
                        slug: r.slug,
                        requests_per_minute: r.requests_per_minute,
                        tokens_per_minute: r.tokens_per_minute,
                        max_concurrent_requests: r.max_concurrent_requests,
                        monthly_token_quota: r.monthly_token_quota,
                        price_multiplier: decimal(&r.price_multiplier, "price_multiplier")?,
                        billing_mode: r.billing_mode,
                        features: json(&r.features_json)?,
                        storage_bytes_quota: r.storage_bytes_quota,
                        max_file_size_bytes: r.max_file_size_bytes,
                        max_files: r.max_files,
                        embeddings_enabled: r.embeddings_enabled,
                        rag_enabled: r.rag_enabled,
                        qdrant_mode: r.qdrant_mode,
                        price_amount: decimal(&r.price_amount, "price_amount")?,
                        currency: r.currency,
                        trial_days: r.trial_days,
                        max_api_keys: r.max_api_keys,
                        overage_enabled: r.overage_enabled,
                        fair_use_policy: r.fair_use_policy,
                        grace_period_days: r.grace_period_days,
                        sso_enabled: r.sso_enabled,
                        sla_enabled: r.sla_enabled,
                        mtls_enabled: r.mtls_enabled,
                        s3_api_enabled: r.s3_api_enabled,
                        max_ingestion_files_per_hour: r.max_ingestion_files_per_hour,
                        max_ingestion_files_per_day: r.max_ingestion_files_per_day,
                        max_ingestion_bytes_per_day: r.max_ingestion_bytes_per_day,
                        max_ingestion_tokens_per_month: r.max_ingestion_tokens_per_month,
                        max_concurrent_ingestions: r.max_concurrent_ingestions,
                        max_pending_ingestion_jobs: r.max_pending_ingestion_jobs,
                    },
                    is_active: r.is_active,
                },
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(PlanResponse {
            plan: Some(plan(value)),
        }))
    }

    async fn deactivate_plan(
        &self,
        request: Request<DeactivatePlanRequest>,
    ) -> Result<Response<PlanResponse>, Status> {
        require_admin(&request, &self.admin_callers)?;
        let value = self
            .deactivate_plan
            .execute(uuid(&request.into_inner().plan_id, "plan_id")?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(PlanResponse {
            plan: Some(plan(value)),
        }))
    }

    async fn get_plan(
        &self,
        request: Request<GetPlanRequest>,
    ) -> Result<Response<PlanResponse>, Status> {
        let value = self
            .get_plan
            .execute(uuid(&request.into_inner().plan_id, "plan_id")?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(PlanResponse {
            plan: Some(plan(value)),
        }))
    }

    async fn list_active_plans(
        &self,
        _request: Request<ListActivePlansRequest>,
    ) -> Result<Response<ListPlansResponse>, Status> {
        let plans = self
            .list_plans
            .execute()
            .await
            .map_err(map_error)?
            .into_iter()
            .map(plan)
            .collect();
        Ok(Response::new(ListPlansResponse { plans }))
    }

    async fn accept_terms(
        &self,
        request: Request<AcceptTermsRequest>,
    ) -> Result<Response<AcceptTermsResponse>, Status> {
        let r = request.into_inner();
        let user_id = uuid(&r.user_id, "user_id")?;
        if r.terms_version.trim().is_empty() || r.privacy_version.trim().is_empty() {
            return Err(Status::invalid_argument(
                "terms_version and privacy_version are required",
            ));
        }
        let ip = (!r.ip_address.trim().is_empty()).then_some(r.ip_address.as_str());
        let country = (!r.country.trim().is_empty()).then_some(r.country.as_str());
        let accepted_at = self
            .create_plan
            .repo
            .accept_terms(
                user_id,
                &r.terms_version,
                &r.privacy_version,
                ip,
                &r.user_agent,
                country,
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(AcceptTermsResponse {
            accepted: true,
            accepted_at: accepted_at.to_rfc3339(),
        }))
    }

    async fn get_terms_status(
        &self,
        request: Request<GetTermsStatusRequest>,
    ) -> Result<Response<GetTermsStatusResponse>, Status> {
        let r = request.into_inner();
        let accepted = self
            .create_plan
            .repo
            .has_terms_acceptance(
                uuid(&r.user_id, "user_id")?,
                &r.terms_version,
                &r.privacy_version,
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(GetTermsStatusResponse {
            accepted,
            terms_version: r.terms_version,
            privacy_version: r.privacy_version,
        }))
    }

    async fn create_subscription(
        &self,
        request: Request<CreateSubscriptionRequest>,
    ) -> Result<Response<SubscriptionResponse>, Status> {
        let r = request.into_inner();
        if r.terms_version != "2026-07-24" || r.privacy_version != "2026-07-24" {
            return Err(Status::failed_precondition(
                "current terms and privacy acceptance required",
            ));
        }
        let user_id = uuid(&r.user_id, "user_id")?;
        if !self
            .create_subscription
            .plans
            .has_terms_acceptance(user_id, &r.terms_version, &r.privacy_version)
            .await
            .map_err(map_error)?
        {
            return Err(Status::failed_precondition(
                "current terms and privacy acceptance required",
            ));
        }
        let value = self
            .create_subscription
            .execute(user_id, uuid(&r.plan_id, "plan_id")?, r.billing_cycle)
            .await
            .map_err(map_error)?;
        Ok(Response::new(SubscriptionResponse {
            subscription: Some(subscription(value)),
        }))
    }

    async fn upgrade_subscription(
        &self,
        request: Request<ChangeSubscriptionPlanRequest>,
    ) -> Result<Response<SubscriptionResponse>, Status> {
        let r = request.into_inner();
        let value = self
            .change_subscription
            .change_plan(uuid(&r.user_id, "user_id")?, uuid(&r.plan_id, "plan_id")?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(SubscriptionResponse {
            subscription: Some(subscription(value)),
        }))
    }

    async fn downgrade_subscription(
        &self,
        request: Request<ChangeSubscriptionPlanRequest>,
    ) -> Result<Response<SubscriptionResponse>, Status> {
        let r = request.into_inner();
        let value = self
            .change_subscription
            .change_plan(uuid(&r.user_id, "user_id")?, uuid(&r.plan_id, "plan_id")?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(SubscriptionResponse {
            subscription: Some(subscription(value)),
        }))
    }

    async fn cancel_subscription(
        &self,
        request: Request<CancelSubscriptionRequest>,
    ) -> Result<Response<SubscriptionResponse>, Status> {
        let r = request.into_inner();
        let value = self
            .change_subscription
            .cancel(uuid(&r.user_id, "user_id")?, r.at_period_end)
            .await
            .map_err(map_error)?;
        Ok(Response::new(SubscriptionResponse {
            subscription: Some(subscription(value)),
        }))
    }

    async fn renew_subscription(
        &self,
        request: Request<RenewSubscriptionRequest>,
    ) -> Result<Response<SubscriptionResponse>, Status> {
        let r = request.into_inner();
        let value = self
            .change_subscription
            .renew(uuid(&r.user_id, "user_id")?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(SubscriptionResponse {
            subscription: Some(subscription(value)),
        }))
    }

    async fn mark_subscription_past_due(
        &self,
        request: Request<MarkSubscriptionPastDueRequest>,
    ) -> Result<Response<SubscriptionResponse>, Status> {
        require_admin(&request, &self.admin_callers)?;
        let value = self
            .change_subscription
            .past_due(uuid(
                &request.into_inner().subscription_id,
                "subscription_id",
            )?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(SubscriptionResponse {
            subscription: Some(subscription(value)),
        }))
    }

    async fn get_active_subscription(
        &self,
        request: Request<GetActiveSubscriptionRequest>,
    ) -> Result<Response<GetSubscriptionResponse>, Status> {
        let value = self
            .get_subscription
            .active_by_user(uuid(&request.into_inner().user_id, "user_id")?)
            .await
            .map_err(map_error)?
            .map(subscription);
        Ok(Response::new(GetSubscriptionResponse {
            subscription: value,
        }))
    }

    async fn get_subscription(
        &self,
        request: Request<GetSubscriptionRequest>,
    ) -> Result<Response<GetSubscriptionResponse>, Status> {
        let value = self
            .get_subscription
            .by_id(uuid(
                &request.into_inner().subscription_id,
                "subscription_id",
            )?)
            .await
            .map_err(map_error)?
            .map(subscription);
        Ok(Response::new(GetSubscriptionResponse {
            subscription: value,
        }))
    }
}
