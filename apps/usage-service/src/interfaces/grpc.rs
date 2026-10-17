use crate::application::service::{UsageApplication, UsageRange};
use crate::domain::entities::{
    ErrorBucket as DomainErrorBucket, ErrorRate as DomainErrorRate,
    LatencyStats as DomainLatencyStats, TopModel as DomainTopModel,
    UsageSummary as DomainUsageSummary,
};
use crate::domain::errors::UsageError;
use chrono::{DateTime, Utc};
use common::ServiceJwtClaims;
use proto::usage_v1::usage_reporting_service_server::UsageReportingService;
use proto::usage_v1::{
    CreateNextMonthPartitionRequest, CreateNextMonthPartitionResponse, CurrentMonthUsageRequest,
    ErrorBucket, ErrorRateRequest, ErrorRateResponse, LatencyStatsByUserRequest,
    LatencyStatsRequest, LatencyStatsResponse, RefreshMaterializedViewsRequest,
    RefreshMaterializedViewsResponse, TimeRange, TopModel, TopModelsByUserRequest,
    TopModelsRequest, TopModelsResponse, UsageByApiKeyRequest, UsageByModelRequest,
    UsageByUserRequest, UsageSummary, UsageSummaryResponse,
};
use std::collections::HashSet;
use std::sync::Arc;
use tonic::{Request, Response, Status};
use uuid::Uuid;

#[derive(Clone)]
pub struct UsageGrpcService {
    pub app: Arc<UsageApplication>,
    pub admin_callers: HashSet<String>,
    pub user_access_callers: HashSet<String>,
    pub aggregate_access_callers: HashSet<String>,
}

fn uuid(raw: &str, field: &str) -> Result<Uuid, Status> {
    Uuid::parse_str(raw).map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
}

fn timestamp(raw: &str, field: &str) -> Result<DateTime<Utc>, Status> {
    DateTime::parse_from_rfc3339(raw)
        .map(|value| value.with_timezone(&Utc))
        .map_err(|e| Status::invalid_argument(format!("invalid {field}: {e}")))
}

fn range(value: Option<TimeRange>) -> Result<UsageRange, Status> {
    let value = value.ok_or_else(|| Status::invalid_argument("range is required"))?;
    Ok(UsageRange {
        start: timestamp(&value.start, "range.start")?,
        end: timestamp(&value.end, "range.end")?,
    })
}

fn map_error(error: UsageError) -> Status {
    match error {
        UsageError::InvalidTimeRange => Status::invalid_argument(error.to_string()),
        UsageError::DuplicateRequestId => Status::already_exists(error.to_string()),
        UsageError::Validation(message) => Status::invalid_argument(message),
        UsageError::DependencyUnavailable(message) => {
            tracing::error!(error = %message, "usage dependency unavailable");
            Status::unavailable("service temporarily unavailable")
        }
    }
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
            "caller is not allowed for usage admin operations",
        ))
    }
}

fn require_user_access<T>(
    request: &Request<T>,
    admin_callers: &HashSet<String>,
    user_access_callers: &HashSet<String>,
) -> Result<(), Status> {
    let claims = request
        .extensions()
        .get::<ServiceJwtClaims>()
        .ok_or_else(|| Status::unauthenticated("missing service claims"))?;
    if admin_callers.contains(&claims.sub) || user_access_callers.contains(&claims.sub) {
        Ok(())
    } else {
        Err(Status::permission_denied(
            "caller is not allowed to access user-scoped usage data",
        ))
    }
}

fn require_aggregate_access<T>(
    request: &Request<T>,
    admin_callers: &HashSet<String>,
    aggregate_access_callers: &HashSet<String>,
) -> Result<(), Status> {
    let claims = request
        .extensions()
        .get::<ServiceJwtClaims>()
        .ok_or_else(|| Status::unauthenticated("missing service claims"))?;
    if admin_callers.contains(&claims.sub) || aggregate_access_callers.contains(&claims.sub) {
        Ok(())
    } else {
        Err(Status::permission_denied(
            "caller is not allowed to access aggregate usage data",
        ))
    }
}

fn summary(value: DomainUsageSummary) -> UsageSummary {
    UsageSummary {
        total_requests: value.total_requests,
        total_input_tokens: value.total_input_tokens,
        total_output_tokens: value.total_output_tokens,
        total_cached_tokens: value.total_cached_tokens,
        error_count: value.error_count,
        error_rate: value.error_rate,
        avg_latency_ms: value.avg_latency_ms,
    }
}

fn top_model(value: DomainTopModel) -> TopModel {
    TopModel {
        model_catalog_id: value.model_catalog_id.to_string(),
        total_requests: value.total_requests,
        total_input_tokens: value.total_input_tokens,
        total_output_tokens: value.total_output_tokens,
        avg_latency_ms: value.avg_latency_ms,
        error_count: value.error_count,
    }
}

fn error_bucket(value: DomainErrorBucket) -> ErrorBucket {
    ErrorBucket {
        error_code: value.error_code,
        count: value.count,
    }
}

fn error_rate(value: DomainErrorRate, buckets: Vec<DomainErrorBucket>) -> ErrorRateResponse {
    ErrorRateResponse {
        total_requests: value.total_requests,
        error_count: value.error_count,
        error_rate: value.error_rate,
        buckets: buckets.into_iter().map(error_bucket).collect(),
    }
}

fn latency(value: DomainLatencyStats) -> LatencyStatsResponse {
    LatencyStatsResponse {
        sample_count: value.sample_count,
        min_latency_ms: value.min_latency_ms,
        avg_latency_ms: value.avg_latency_ms,
        p50_latency_ms: value.p50_latency_ms,
        p95_latency_ms: value.p95_latency_ms,
        p99_latency_ms: value.p99_latency_ms,
        max_latency_ms: value.max_latency_ms,
    }
}

#[tonic::async_trait]
impl UsageReportingService for UsageGrpcService {
    async fn get_usage_by_user(
        &self,
        request: Request<UsageByUserRequest>,
    ) -> Result<Response<UsageSummaryResponse>, Status> {
        require_user_access(&request, &self.admin_callers, &self.user_access_callers)?;
        let r = request.into_inner();
        let value = self
            .app
            .usage_by_user(uuid(&r.user_id, "user_id")?, range(r.range)?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(UsageSummaryResponse {
            summary: Some(summary(value)),
        }))
    }

    async fn get_usage_by_api_key(
        &self,
        request: Request<UsageByApiKeyRequest>,
    ) -> Result<Response<UsageSummaryResponse>, Status> {
        require_user_access(&request, &self.admin_callers, &self.user_access_callers)?;
        let r = request.into_inner();
        let value = self
            .app
            .usage_by_api_key(uuid(&r.api_key_id, "api_key_id")?, range(r.range)?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(UsageSummaryResponse {
            summary: Some(summary(value)),
        }))
    }

    async fn get_usage_by_model(
        &self,
        request: Request<UsageByModelRequest>,
    ) -> Result<Response<UsageSummaryResponse>, Status> {
        require_aggregate_access(
            &request,
            &self.admin_callers,
            &self.aggregate_access_callers,
        )?;
        let r = request.into_inner();
        let value = self
            .app
            .usage_by_model(
                uuid(&r.model_catalog_id, "model_catalog_id")?,
                range(r.range)?,
            )
            .await
            .map_err(map_error)?;
        Ok(Response::new(UsageSummaryResponse {
            summary: Some(summary(value)),
        }))
    }

    async fn get_current_month_usage(
        &self,
        request: Request<CurrentMonthUsageRequest>,
    ) -> Result<Response<UsageSummaryResponse>, Status> {
        require_user_access(&request, &self.admin_callers, &self.user_access_callers)?;
        let value = self
            .app
            .current_month_usage(uuid(&request.into_inner().user_id, "user_id")?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(UsageSummaryResponse {
            summary: Some(summary(value)),
        }))
    }

    async fn get_top_models(
        &self,
        request: Request<TopModelsRequest>,
    ) -> Result<Response<TopModelsResponse>, Status> {
        require_aggregate_access(
            &request,
            &self.admin_callers,
            &self.aggregate_access_callers,
        )?;
        let r = request.into_inner();
        let models = self
            .app
            .top_models(range(r.range)?, i64::from(r.limit))
            .await
            .map_err(map_error)?
            .into_iter()
            .map(top_model)
            .collect();
        Ok(Response::new(TopModelsResponse { models }))
    }

    async fn get_top_models_by_user(
        &self,
        request: Request<TopModelsByUserRequest>,
    ) -> Result<Response<TopModelsResponse>, Status> {
        require_user_access(&request, &self.admin_callers, &self.user_access_callers)?;
        let value = request.into_inner();
        let models = self
            .app
            .top_models_for_user(
                uuid(&value.user_id, "user_id")?,
                range(value.range)?,
                i64::from(value.limit),
            )
            .await
            .map_err(map_error)?
            .into_iter()
            .map(top_model)
            .collect();
        Ok(Response::new(TopModelsResponse { models }))
    }

    async fn get_error_rate(
        &self,
        request: Request<ErrorRateRequest>,
    ) -> Result<Response<ErrorRateResponse>, Status> {
        require_aggregate_access(
            &request,
            &self.admin_callers,
            &self.aggregate_access_callers,
        )?;
        let (rate, buckets) = self
            .app
            .error_rate(range(request.into_inner().range)?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(error_rate(rate, buckets)))
    }

    async fn get_latency_stats(
        &self,
        request: Request<LatencyStatsRequest>,
    ) -> Result<Response<LatencyStatsResponse>, Status> {
        require_aggregate_access(
            &request,
            &self.admin_callers,
            &self.aggregate_access_callers,
        )?;
        let value = self
            .app
            .latency_stats(range(request.into_inner().range)?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(latency(value)))
    }

    async fn get_latency_stats_by_user(
        &self,
        request: Request<LatencyStatsByUserRequest>,
    ) -> Result<Response<LatencyStatsResponse>, Status> {
        require_user_access(&request, &self.admin_callers, &self.user_access_callers)?;
        let value = request.into_inner();
        let stats = self
            .app
            .latency_stats_for_user(uuid(&value.user_id, "user_id")?, range(value.range)?)
            .await
            .map_err(map_error)?;
        Ok(Response::new(latency(stats)))
    }

    async fn refresh_materialized_views(
        &self,
        request: Request<RefreshMaterializedViewsRequest>,
    ) -> Result<Response<RefreshMaterializedViewsResponse>, Status> {
        require_admin(&request, &self.admin_callers)?;
        self.app
            .refresh_materialized_views()
            .await
            .map_err(map_error)?;
        Ok(Response::new(RefreshMaterializedViewsResponse {
            refreshed: true,
        }))
    }

    async fn create_next_month_partition(
        &self,
        request: Request<CreateNextMonthPartitionRequest>,
    ) -> Result<Response<CreateNextMonthPartitionResponse>, Status> {
        require_admin(&request, &self.admin_callers)?;
        let partition = self
            .app
            .create_next_month_partition()
            .await
            .map_err(map_error)?;
        Ok(Response::new(CreateNextMonthPartitionResponse {
            partition_name: partition,
            created_or_verified: true,
        }))
    }
}
