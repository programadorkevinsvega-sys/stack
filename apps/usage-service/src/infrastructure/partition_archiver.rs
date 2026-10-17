use crate::domain::errors::UsageError;
use async_trait::async_trait;
use chrono::{DateTime, Datelike, Utc};
use sqlx::PgPool;

#[async_trait]
pub trait PartitionManager: Send + Sync {
    async fn archive_old_partitions(
        &self,
        retention_months: u32,
        bucket: &str,
    ) -> Result<Vec<String>, UsageError>;
}

#[derive(Clone)]
pub struct R2PartitionArchiver {
    pool: PgPool,
    s3_client: aws_sdk_s3::Client,
}

impl R2PartitionArchiver {
    pub async fn new(
        pool: PgPool,
        r2_endpoint: &str,
        r2_access_key: &str,
        r2_secret_key: &str,
    ) -> Self {
        let creds =
            aws_sdk_s3::config::Credentials::new(r2_access_key, r2_secret_key, None, None, "r2");
        let provider = aws_sdk_s3::config::SharedCredentialsProvider::new(creds);
        let s3_conf = aws_sdk_s3::Config::builder()
            .endpoint_url(r2_endpoint)
            .credentials_provider(provider)
            .region(aws_sdk_s3::config::Region::new("auto"))
            .behavior_version(aws_sdk_s3::config::BehaviorVersion::latest())
            .build();
        Self {
            pool,
            s3_client: aws_sdk_s3::Client::from_conf(s3_conf),
        }
    }

    pub async fn verify_bucket(&self, bucket: &str) -> Result<(), UsageError> {
        if let Err(error) = self.s3_client.head_bucket().bucket(bucket).send().await {
            tracing::error!(
                dependency = "r2",
                bucket,
                http_status = error
                    .raw_response()
                    .map(|response| response.status().as_u16()),
                error = ?error,
                "R2 bucket health check request failed"
            );
            return Err(UsageError::DependencyUnavailable(format!(
                "R2 bucket health check failed: {error}"
            )));
        }
        Ok(())
    }
}

#[async_trait]
impl PartitionManager for R2PartitionArchiver {
    async fn archive_old_partitions(
        &self,
        retention_months: u32,
        bucket: &str,
    ) -> Result<Vec<String>, UsageError> {
        let cutoff = Utc::now()
            .checked_sub_months(chrono::Months::new(retention_months))
            .unwrap_or(Utc::now());

        let partitions = list_old_partitions(&self.pool, cutoff).await?;
        if partitions.is_empty() {
            tracing::info!("no old partitions to archive");
            return Ok(Vec::new());
        }

        let mut archived = Vec::new();
        for partition in &partitions {
            match archive_single_partition(&self.pool, &self.s3_client, partition, bucket).await {
                Ok(key) => {
                    drop_partition(&self.pool, &partition.name).await?;
                    tracing::info!(
                        partition = %partition.name,
                        s3_key = %key,
                        "partition archived and dropped"
                    );
                    archived.push(partition.name.clone());
                }
                Err(e) => {
                    tracing::error!(
                        partition = %partition.name,
                        error = %e,
                        "failed to archive partition"
                    );
                }
            }
        }

        Ok(archived)
    }
}

struct PartitionInfo {
    name: String,
    range_start: DateTime<Utc>,
}

async fn list_old_partitions(
    pool: &PgPool,
    cutoff: DateTime<Utc>,
) -> Result<Vec<PartitionInfo>, UsageError> {
    let rows = sqlx::query!(
        r#"SELECT c.relname AS name,
                  pg_catalog.pg_get_expr(c.relpartbound, c.oid, true) AS range_expr
           FROM pg_catalog.pg_inherits i
           JOIN pg_catalog.pg_class c ON c.oid = i.inhrelid
           JOIN pg_catalog.pg_class p ON p.oid = i.inhparent
           WHERE p.relname = 'usage_events'
             AND p.relnamespace = (SELECT oid FROM pg_catalog.pg_namespace WHERE nspname = 'public')
           ORDER BY c.relname"#
    )
    .fetch_all(pool)
    .await
    .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))?;

    let mut partitions = Vec::new();
    for row in rows {
        let name = row.name;
        let range_expr = row.range_expr.unwrap_or_default();
        let start = parse_partition_start(&range_expr);
        if let Some(start) = start {
            if start < cutoff {
                partitions.push(PartitionInfo {
                    name,
                    range_start: start,
                });
            }
        }
    }
    Ok(partitions)
}

fn parse_partition_start(expr: &str) -> Option<DateTime<Utc>> {
    let start = expr
        .split("FROM (")
        .nth(1)?
        .split(')')
        .next()?
        .trim_matches('\'');
    chrono::NaiveDate::parse_from_str(start, "%Y-%m-%d")
        .ok()
        .and_then(|d| {
            d.and_hms_opt(0, 0, 0)
                .map(|dt| DateTime::<Utc>::from_naive_utc_and_offset(dt, Utc))
        })
}

async fn archive_single_partition(
    pool: &PgPool,
    s3: &aws_sdk_s3::Client,
    partition: &PartitionInfo,
    bucket: &str,
) -> Result<String, UsageError> {
    let csv = export_partition_rows(pool, &partition.name).await?;
    let year = partition.range_start.year();
    let month = partition.range_start.month();
    let key = format!("usage-archive/{year:04}/{month:02}/{}.csv", partition.name);

    s3.put_object()
        .bucket(bucket)
        .key(&key)
        .body(csv.into())
        .content_type("text/csv")
        .send()
        .await
        .map_err(|e| UsageError::DependencyUnavailable(format!("r2 upload failed: {e}")))?;

    Ok(key)
}

#[derive(sqlx::FromRow)]
struct PartitionRow {
    id: uuid::Uuid,
    request_id: uuid::Uuid,
    api_key_id: uuid::Uuid,
    user_id: uuid::Uuid,
    model_catalog_id: uuid::Uuid,
    aws_account_id: uuid::Uuid,
    input_tokens: i32,
    output_tokens: i32,
    cached_tokens: i32,
    stream: bool,
    latency_ms: i32,
    ttft_ms: Option<i32>,
    status_code: i16,
    error_code: Option<String>,
    created_at: chrono::NaiveDateTime,
}

async fn export_partition_rows(pool: &PgPool, partition_name: &str) -> Result<Vec<u8>, UsageError> {
    let sql = format!(
        "SELECT id, request_id, api_key_id, user_id, model_catalog_id, aws_account_id,
                input_tokens, output_tokens, cached_tokens, stream, latency_ms,
                ttft_ms, status_code, error_code, created_at
         FROM {partition_name}
         ORDER BY created_at"
    );
    let rows: Vec<PartitionRow> = sqlx::query_as::<_, PartitionRow>(&sql)
        .fetch_all(pool)
        .await
        .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))?;

    let mut csv = String::from(
        "id,request_id,api_key_id,user_id,model_catalog_id,aws_account_id,\
         input_tokens,output_tokens,cached_tokens,stream,latency_ms,\
         ttft_ms,status_code,error_code,created_at\n",
    );
    for row in &rows {
        csv.push_str(&format!(
            "{},{},{},{},{},{},{},{},{},{},{},{},{},{},{}\n",
            row.id,
            row.request_id,
            row.api_key_id,
            row.user_id,
            row.model_catalog_id,
            row.aws_account_id,
            row.input_tokens,
            row.output_tokens,
            row.cached_tokens,
            row.stream,
            row.latency_ms,
            row.ttft_ms.map_or("".to_string(), |v| v.to_string()),
            row.status_code,
            row.error_code.as_deref().unwrap_or(""),
            row.created_at,
        ));
    }

    Ok(csv.into_bytes())
}

async fn drop_partition(pool: &PgPool, partition_name: &str) -> Result<(), UsageError> {
    let sql = format!("DROP TABLE IF EXISTS {partition_name}");
    sqlx::query(&sql)
        .execute(pool)
        .await
        .map_err(|e| UsageError::DependencyUnavailable(e.to_string()))?;
    Ok(())
}
