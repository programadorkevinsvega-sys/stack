use crate::application::service::UsageRange;
use crate::domain::entities::{
    ErrorBucket, ErrorRate, LatencyStats, TopModel, UsageEvent, UsageSummary,
};
use crate::domain::errors::UsageError;
use chrono::{DateTime, Datelike, TimeZone, Utc};
use sqlx::PgPool;

#[derive(Clone)]
pub struct PgUsageRepository {
    pub pool: PgPool,
}

impl PgUsageRepository {
    pub async fn save_event(&self, event: &UsageEvent) -> Result<bool, UsageError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))?;
        let request_key = format!("usage_event:{}", event.request_id);
        sqlx::query!("SELECT pg_advisory_xact_lock(hashtext($1))", &request_key)
            .execute(tx.as_mut())
            .await
            .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))?;
        ensure_month_partition_in_tx(tx.as_mut(), event.created_at).await?;
        let exists = sqlx::query_scalar!(
            r#"SELECT EXISTS(SELECT 1 FROM usage_events WHERE request_id=$1) AS "exists!""#,
            event.request_id
        )
        .fetch_one(tx.as_mut())
        .await
        .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))?;
        if exists {
            tx.commit()
                .await
                .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))?;
            return Ok(false);
        }
        sqlx::query!(
            "INSERT INTO usage_events (id,request_id,api_key_id,user_id,model_catalog_id,aws_account_id,input_tokens,output_tokens,cached_tokens,stream,latency_ms,ttft_ms,status_code,error_code,created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
            event.id,
            event.request_id,
            event.api_key_id,
            event.user_id,
            event.model_catalog_id,
            event.aws_account_id,
            event.input_tokens,
            event.output_tokens,
            event.cached_tokens,
            event.stream,
            event.latency_ms,
            event.ttft_ms,
            event.status_code,
            event.error_code,
            event.created_at,
        )
        .execute(tx.as_mut())
        .await
        .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))?;
        tx.commit()
            .await
            .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))?;
        Ok(true)
    }

    pub async fn summary_by_user(
        &self,
        user_id: uuid::Uuid,
        range: UsageRange,
    ) -> Result<UsageSummary, UsageError> {
        summary_with_filter(
            &self.pool,
            "user_id=$1 AND created_at >= $2 AND created_at < $3",
            user_id,
            range,
        )
        .await
    }

    pub async fn summary_by_api_key(
        &self,
        api_key_id: uuid::Uuid,
        range: UsageRange,
    ) -> Result<UsageSummary, UsageError> {
        summary_with_filter(
            &self.pool,
            "api_key_id=$1 AND created_at >= $2 AND created_at < $3",
            api_key_id,
            range,
        )
        .await
    }

    pub async fn summary_by_model(
        &self,
        model_catalog_id: uuid::Uuid,
        range: UsageRange,
    ) -> Result<UsageSummary, UsageError> {
        summary_with_filter(
            &self.pool,
            "model_catalog_id=$1 AND created_at >= $2 AND created_at < $3",
            model_catalog_id,
            range,
        )
        .await
    }

    pub async fn top_models(
        &self,
        range: UsageRange,
        limit: i64,
    ) -> Result<Vec<TopModel>, UsageError> {
        sqlx::query_as::<_, TopModel>(
            "SELECT model_catalog_id,
                COUNT(*)::BIGINT AS total_requests,
                COALESCE(SUM(input_tokens),0)::BIGINT AS total_input_tokens,
                COALESCE(SUM(output_tokens),0)::BIGINT AS total_output_tokens,
                COALESCE(AVG(latency_ms),0)::FLOAT8 AS avg_latency_ms,
                COUNT(*) FILTER (WHERE status_code >= 400)::BIGINT AS error_count
             FROM usage_events
             WHERE created_at >= $1 AND created_at < $2
             GROUP BY model_catalog_id
             ORDER BY total_requests DESC,
                      COALESCE(SUM(input_tokens), 0) + COALESCE(SUM(output_tokens), 0) DESC
             LIMIT $3",
        )
        .bind(range.start)
        .bind(range.end)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))
    }

    pub async fn top_models_for_user(
        &self,
        user_id: uuid::Uuid,
        range: UsageRange,
        limit: i64,
    ) -> Result<Vec<TopModel>, UsageError> {
        sqlx::query_as::<_, TopModel>(
            "SELECT model_catalog_id,
                COUNT(*)::BIGINT AS total_requests,
                COALESCE(SUM(input_tokens),0)::BIGINT AS total_input_tokens,
                COALESCE(SUM(output_tokens),0)::BIGINT AS total_output_tokens,
                COALESCE(AVG(latency_ms),0)::FLOAT8 AS avg_latency_ms,
                COUNT(*) FILTER (WHERE status_code >= 400)::BIGINT AS error_count
             FROM usage_events
             WHERE user_id = $1 AND created_at >= $2 AND created_at < $3
             GROUP BY model_catalog_id
             ORDER BY total_requests DESC,
                      COALESCE(SUM(input_tokens), 0) + COALESCE(SUM(output_tokens), 0) DESC
             LIMIT $4",
        )
        .bind(user_id)
        .bind(range.start)
        .bind(range.end)
        .bind(limit.clamp(1, 100))
        .fetch_all(&self.pool)
        .await
        .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))
    }

    pub async fn error_rate(
        &self,
        range: UsageRange,
    ) -> Result<(ErrorRate, Vec<ErrorBucket>), UsageError> {
        let rate = sqlx::query_as::<_, ErrorRate>(
            "SELECT
                COUNT(*)::BIGINT AS total_requests,
                COUNT(*) FILTER (WHERE status_code >= 400)::BIGINT AS error_count,
                CASE WHEN COUNT(*) = 0 THEN 0 ELSE COUNT(*) FILTER (WHERE status_code >= 400)::FLOAT8 / COUNT(*)::FLOAT8 END AS error_rate
             FROM usage_events
             WHERE created_at >= $1 AND created_at < $2",
        )
        .bind(range.start)
        .bind(range.end)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))?;
        let buckets = sqlx::query_as::<_, ErrorBucket>(
            "SELECT COALESCE(error_code, status_code::TEXT) AS error_code,
                COUNT(*)::BIGINT AS count
             FROM usage_events
             WHERE created_at >= $1 AND created_at < $2 AND status_code >= 400
             GROUP BY COALESCE(error_code, status_code::TEXT)
             ORDER BY count DESC",
        )
        .bind(range.start)
        .bind(range.end)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))?;
        Ok((rate, buckets))
    }

    pub async fn latency_stats(&self, range: UsageRange) -> Result<LatencyStats, UsageError> {
        sqlx::query_as::<_, LatencyStats>(
            "SELECT
                COUNT(*)::BIGINT AS sample_count,
                COALESCE(MIN(latency_ms),0)::FLOAT8 AS min_latency_ms,
                COALESCE(AVG(latency_ms),0)::FLOAT8 AS avg_latency_ms,
                COALESCE(percentile_cont(0.50) WITHIN GROUP (ORDER BY latency_ms),0)::FLOAT8 AS p50_latency_ms,
                COALESCE(percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms),0)::FLOAT8 AS p95_latency_ms,
                COALESCE(percentile_cont(0.99) WITHIN GROUP (ORDER BY latency_ms),0)::FLOAT8 AS p99_latency_ms,
                COALESCE(MAX(latency_ms),0)::FLOAT8 AS max_latency_ms
             FROM usage_events
             WHERE created_at >= $1 AND created_at < $2",
        )
        .bind(range.start)
        .bind(range.end)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))
    }

    pub async fn latency_stats_for_user(
        &self,
        user_id: uuid::Uuid,
        range: UsageRange,
    ) -> Result<LatencyStats, UsageError> {
        sqlx::query_as::<_, LatencyStats>(
            "SELECT
                COUNT(*)::BIGINT AS sample_count,
                COALESCE(MIN(latency_ms),0)::FLOAT8 AS min_latency_ms,
                COALESCE(AVG(latency_ms),0)::FLOAT8 AS avg_latency_ms,
                COALESCE(percentile_cont(0.50) WITHIN GROUP (ORDER BY latency_ms),0)::FLOAT8 AS p50_latency_ms,
                COALESCE(percentile_cont(0.95) WITHIN GROUP (ORDER BY latency_ms),0)::FLOAT8 AS p95_latency_ms,
                COALESCE(percentile_cont(0.99) WITHIN GROUP (ORDER BY latency_ms),0)::FLOAT8 AS p99_latency_ms,
                COALESCE(MAX(latency_ms),0)::FLOAT8 AS max_latency_ms
             FROM usage_events
             WHERE user_id = $1 AND created_at >= $2 AND created_at < $3",
        )
        .bind(user_id)
        .bind(range.start)
        .bind(range.end)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))
    }

    pub async fn refresh_materialized_views(&self) -> Result<(), UsageError> {
        refresh_view(&self.pool, "usage_hourly").await?;
        refresh_view(&self.pool, "usage_daily").await?;
        Ok(())
    }

    pub async fn ensure_next_month_partition(&self) -> Result<String, UsageError> {
        let now = Utc::now();
        let current_month = Utc
            .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
            .single()
            .ok_or_else(|| UsageError::Validation("invalid current month".into()))?;
        let first_next = add_month(current_month)?;
        ensure_month_partition(&self.pool, first_next).await
    }
}

async fn refresh_view(pool: &PgPool, view_name: &str) -> Result<(), UsageError> {
    let concurrent = format!("REFRESH MATERIALIZED VIEW CONCURRENTLY {view_name}");
    match sqlx::query(&concurrent).execute(pool).await {
        Ok(_) => Ok(()),
        Err(error) if error.to_string().contains("not populated") => {
            let non_concurrent = format!("REFRESH MATERIALIZED VIEW {view_name}");
            sqlx::query(&non_concurrent)
                .execute(pool)
                .await
                .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))?;
            Ok(())
        }
        Err(error) => Err(UsageError::DependencyUnavailable(error.to_string())),
    }
}

async fn summary_with_filter(
    pool: &PgPool,
    filter: &str,
    id: uuid::Uuid,
    range: UsageRange,
) -> Result<UsageSummary, UsageError> {
    let allowed = ["user_id=$1", "api_key_id=$1", "model_catalog_id=$1"];
    if !allowed.iter().any(|a| filter.starts_with(a)) {
        return Err(UsageError::Validation("invalid filter parameter".into()));
    }
    let sql = format!(
        "SELECT
            COUNT(*)::BIGINT AS total_requests,
            COALESCE(SUM(input_tokens),0)::BIGINT AS total_input_tokens,
            COALESCE(SUM(output_tokens),0)::BIGINT AS total_output_tokens,
            COALESCE(SUM(cached_tokens),0)::BIGINT AS total_cached_tokens,
            COUNT(*) FILTER (WHERE status_code >= 400)::BIGINT AS error_count,
            CASE WHEN COUNT(*) = 0 THEN 0 ELSE COUNT(*) FILTER (WHERE status_code >= 400)::FLOAT8 / COUNT(*)::FLOAT8 END AS error_rate,
            COALESCE(AVG(latency_ms),0)::FLOAT8 AS avg_latency_ms
         FROM usage_events
         WHERE {filter} AND created_at >= $2 AND created_at < $3"
    );
    sqlx::query_as::<_, UsageSummary>(&sql)
        .bind(id)
        .bind(range.start)
        .bind(range.end)
        .fetch_one(pool)
        .await
        .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))
}

async fn ensure_month_partition(pool: &PgPool, month: DateTime<Utc>) -> Result<String, UsageError> {
    let name = partition_name(month);
    let start = month_start(month)?;
    let end = add_month(start)?;
    create_partition(pool, &name, start, end).await?;
    Ok(name)
}

async fn ensure_month_partition_in_tx<'e, E>(
    executor: E,
    month: DateTime<Utc>,
) -> Result<(), UsageError>
where
    E: sqlx::Executor<'e, Database = sqlx::Postgres>,
{
    let name = partition_name(month);
    let start = month_start(month)?;
    let end = add_month(start)?;
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {name} PARTITION OF usage_events FOR VALUES FROM ($1) TO ($2)"
    );
    sqlx::query(&sql)
        .bind(start)
        .bind(end)
        .execute(executor)
        .await
        .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))?;
    Ok(())
}

async fn create_partition(
    pool: &PgPool,
    name: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<(), UsageError> {
    let sql = format!(
        "CREATE TABLE IF NOT EXISTS {name} PARTITION OF usage_events FOR VALUES FROM ($1) TO ($2)"
    );
    sqlx::query(&sql)
        .bind(start)
        .bind(end)
        .execute(pool)
        .await
        .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))?;
    Ok(())
}

fn partition_name(month: DateTime<Utc>) -> String {
    format!("usage_events_{:04}_{:02}", month.year(), month.month())
}

fn month_start(value: DateTime<Utc>) -> Result<DateTime<Utc>, UsageError> {
    Utc.with_ymd_and_hms(value.year(), value.month(), 1, 0, 0, 0)
        .single()
        .ok_or_else(|| UsageError::Validation("invalid partition month".into()))
}

fn add_month(value: DateTime<Utc>) -> Result<DateTime<Utc>, UsageError> {
    let year = if value.month() == 12 {
        value.year() + 1
    } else {
        value.year()
    };
    let month = if value.month() == 12 {
        1
    } else {
        value.month() + 1
    };
    Utc.with_ymd_and_hms(year, month, 1, 0, 0, 0)
        .single()
        .ok_or_else(|| UsageError::Validation("invalid next partition month".into()))
}
