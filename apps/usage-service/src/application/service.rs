use crate::domain::entities::{
    ErrorBucket, ErrorRate, LatencyStats, TopModel, UsageEvent, UsageSummary,
};
use crate::domain::errors::UsageError;
use crate::infrastructure::persistence::PgUsageRepository;
use chrono::{DateTime, Datelike, Duration, TimeZone, Utc};
use events::UsageRecorded;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Clone)]
pub struct UsageApplication {
    pub repo: Arc<PgUsageRepository>,
}

#[derive(Clone, Copy)]
pub struct UsageRange {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl UsageRange {
    pub fn validate(self) -> Result<Self, UsageError> {
        if self.start >= self.end {
            return Err(UsageError::InvalidTimeRange);
        }
        Ok(self)
    }
}

impl UsageApplication {
    pub async fn record_usage_event(
        &self,
        event: UsageRecorded,
        recorded_at: DateTime<Utc>,
    ) -> Result<bool, UsageError> {
        validate_recorded_event(&event)?;
        let usage = UsageEvent {
            id: Uuid::now_v7(),
            request_id: event.request_id,
            api_key_id: event.api_key_id,
            user_id: event.user_id,
            model_catalog_id: event.model_catalog_id,
            aws_account_id: event.aws_account_id,
            input_tokens: i32::try_from(event.input_tokens).unwrap_or(i32::MAX),
            output_tokens: i32::try_from(event.output_tokens).unwrap_or(i32::MAX),
            cached_tokens: i32::try_from(event.cached_tokens).unwrap_or(i32::MAX),
            stream: event.stream,
            latency_ms: i32::try_from(event.latency_ms).unwrap_or(i32::MAX),
            ttft_ms: event.ttft_ms.map(|value| value as i32),
            status_code: event.status_code as i16,
            error_code: event.error_code,
            created_at: clamp_timestamp(recorded_at),
        };
        self.repo.save_event(&usage).await
    }

    pub async fn usage_by_user(
        &self,
        user_id: Uuid,
        range: UsageRange,
    ) -> Result<UsageSummary, UsageError> {
        self.repo.summary_by_user(user_id, range.validate()?).await
    }

    pub async fn usage_by_api_key(
        &self,
        api_key_id: Uuid,
        range: UsageRange,
    ) -> Result<UsageSummary, UsageError> {
        self.repo
            .summary_by_api_key(api_key_id, range.validate()?)
            .await
    }

    pub async fn usage_by_model(
        &self,
        model_catalog_id: Uuid,
        range: UsageRange,
    ) -> Result<UsageSummary, UsageError> {
        self.repo
            .summary_by_model(model_catalog_id, range.validate()?)
            .await
    }

    pub async fn current_month_usage(&self, user_id: Uuid) -> Result<UsageSummary, UsageError> {
        let now = Utc::now();
        let start = Utc
            .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
            .single()
            .ok_or_else(|| UsageError::Validation("invalid current month".into()))?;
        let end = if now.month() == 12 {
            Utc.with_ymd_and_hms(now.year() + 1, 1, 1, 0, 0, 0)
        } else {
            Utc.with_ymd_and_hms(now.year(), now.month() + 1, 1, 0, 0, 0)
        }
        .single()
        .ok_or_else(|| UsageError::Validation("invalid next month".into()))?;
        self.usage_by_user(user_id, UsageRange { start, end }).await
    }

    pub async fn top_models(
        &self,
        range: UsageRange,
        limit: i64,
    ) -> Result<Vec<TopModel>, UsageError> {
        self.repo.top_models(range.validate()?, limit).await
    }

    pub async fn top_models_for_user(
        &self,
        user_id: Uuid,
        range: UsageRange,
        limit: i64,
    ) -> Result<Vec<TopModel>, UsageError> {
        self.repo
            .top_models_for_user(user_id, range.validate()?, limit)
            .await
    }

    pub async fn error_rate(
        &self,
        range: UsageRange,
    ) -> Result<(ErrorRate, Vec<ErrorBucket>), UsageError> {
        self.repo.error_rate(range.validate()?).await
    }

    pub async fn latency_stats(&self, range: UsageRange) -> Result<LatencyStats, UsageError> {
        self.repo.latency_stats(range.validate()?).await
    }

    pub async fn latency_stats_for_user(
        &self,
        user_id: Uuid,
        range: UsageRange,
    ) -> Result<LatencyStats, UsageError> {
        self.repo
            .latency_stats_for_user(user_id, range.validate()?)
            .await
    }

    pub async fn refresh_materialized_views(&self) -> Result<(), UsageError> {
        self.repo.refresh_materialized_views().await
    }

    pub async fn create_next_month_partition(&self) -> Result<String, UsageError> {
        self.repo.ensure_next_month_partition().await
    }
}

fn clamp_timestamp(ts: DateTime<Utc>) -> DateTime<Utc> {
    let now = Utc::now();
    let past_limit = now - Duration::days(365);
    let future_limit = now + Duration::hours(24);
    if ts < past_limit {
        past_limit
    } else if ts > future_limit {
        future_limit
    } else {
        ts
    }
}

fn validate_recorded_event(event: &UsageRecorded) -> Result<(), UsageError> {
    if event.status_code < 100 || event.status_code > 599 {
        return Err(UsageError::Validation(
            "status_code must be HTTP-like".into(),
        ));
    }
    if let Some(error_code) = &event.error_code {
        if error_code.len() > 50 {
            return Err(UsageError::Validation(
                "error_code cannot exceed 50 characters".into(),
            ));
        }
    }
    Ok(())
}
