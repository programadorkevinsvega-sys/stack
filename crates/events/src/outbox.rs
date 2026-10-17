use crate::jetstream::JetStreamError;
use crate::ALL_EVENT_SUBJECTS;
use async_nats::jetstream;
use async_nats::HeaderMap;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use std::time::Duration;
use thiserror::Error;
use tokio::time::MissedTickBehavior;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum OutboxError {
    #[error("invalid outbox table name: {0}")]
    InvalidTable(String),
    #[error("unknown canonical event subject: {0}")]
    InvalidSubject(String),
    #[error("database operation failed: {0}")]
    Database(#[from] sqlx::Error),
    #[error("event serialization failed: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error(transparent)]
    JetStream(#[from] JetStreamError),
    #[error("JetStream publish failed: {0}")]
    Publish(String),
}

#[derive(Debug, Clone)]
pub struct OutboxConfig {
    pub table: String,
    pub service: String,
    pub poll_interval: Duration,
    pub batch_size: i64,
    pub max_attempts: i32,
    pub lease_duration: Duration,
}

impl OutboxConfig {
    pub fn for_service(service: impl Into<String>, table: impl Into<String>) -> Self {
        Self {
            table: table.into(),
            service: service.into(),
            poll_interval: Duration::from_millis(env_u64("OUTBOX_POLL_INTERVAL_MS", 500)),
            batch_size: env_i64("OUTBOX_BATCH_SIZE", 100).clamp(1, 1_000),
            max_attempts: env_i64("OUTBOX_MAX_ATTEMPTS", 10).clamp(1, 100) as i32,
            lease_duration: Duration::from_secs(env_u64("OUTBOX_LEASE_SECONDS", 60)),
        }
    }

    fn validate(&self) -> Result<(), OutboxError> {
        validate_table(&self.table)?;
        if self.service.is_empty()
            || !self
                .service
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
        {
            return Err(OutboxError::InvalidTable(self.service.clone()));
        }
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct OutboxStore {
    pool: PgPool,
    table: String,
}

impl OutboxStore {
    pub fn new(pool: PgPool, table: impl Into<String>) -> Result<Self, OutboxError> {
        let table = table.into();
        validate_table(&table)?;
        Ok(Self { pool, table })
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn enqueue<T: Serialize>(
        &self,
        subject: &str,
        envelope: &T,
    ) -> Result<Uuid, OutboxError> {
        validate_subject(subject)?;
        let payload = serde_json::to_value(envelope)?;
        enqueue_on_pool(&self.pool, &self.table, subject, &payload).await
    }

    pub async fn enqueue_value(&self, subject: &str, payload: &Value) -> Result<Uuid, OutboxError> {
        validate_subject(subject)?;
        enqueue_on_pool(&self.pool, &self.table, subject, payload).await
    }
}

pub async fn enqueue_in_transaction<T: Serialize>(
    tx: &mut Transaction<'_, Postgres>,
    table: &str,
    subject: &str,
    envelope: &T,
) -> Result<Uuid, OutboxError> {
    validate_table(table)?;
    validate_subject(subject)?;
    let id = Uuid::now_v7();
    let payload = serde_json::to_value(envelope)?;
    let query = format!(
        "INSERT INTO {table} (id,event_type,payload,status,attempts,available_at,created_at) \
         VALUES ($1,$2,$3,'pending',0,now(),now())"
    );
    sqlx::query(&query)
        .bind(id)
        .bind(subject)
        .bind(payload)
        .execute(tx.as_mut())
        .await?;
    metrics::counter!("outbox_enqueued_total", "subject" => subject.to_string()).increment(1);
    Ok(id)
}

async fn enqueue_on_pool(
    pool: &PgPool,
    table: &str,
    subject: &str,
    payload: &Value,
) -> Result<Uuid, OutboxError> {
    let id = Uuid::now_v7();
    let query = format!(
        "INSERT INTO {table} (id,event_type,payload,status,attempts,available_at,created_at) \
         VALUES ($1,$2,$3,'pending',0,now(),now())"
    );
    sqlx::query(&query)
        .bind(id)
        .bind(subject)
        .bind(payload)
        .execute(pool)
        .await?;
    metrics::counter!("outbox_enqueued_total", "subject" => subject.to_string()).increment(1);
    Ok(id)
}

#[derive(Debug, sqlx::FromRow)]
struct ClaimedEvent {
    id: Uuid,
    event_type: String,
    payload: Value,
    attempts: i32,
    created_at: DateTime<Utc>,
}

pub struct OutboxRelay {
    pool: PgPool,
    context: jetstream::Context,
    config: OutboxConfig,
}

impl OutboxRelay {
    pub fn new(
        pool: PgPool,
        context: jetstream::Context,
        config: OutboxConfig,
    ) -> Result<Self, OutboxError> {
        config.validate()?;
        Ok(Self {
            pool,
            context,
            config,
        })
    }

    pub async fn run(self) {
        let mut interval = tokio::time::interval(self.config.poll_interval);
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        tracing::info!(
            service = %self.config.service,
            table = %self.config.table,
            "outbox relay started"
        );
        loop {
            interval.tick().await;
            if let Err(error) = self.relay_batch().await {
                tracing::error!(
                    service = %self.config.service,
                    %error,
                    "outbox relay batch failed"
                );
                metrics::counter!("outbox_relay_errors_total", "service" => self.config.service.clone())
                    .increment(1);
            }
        }
    }

    pub async fn relay_batch(&self) -> Result<usize, OutboxError> {
        let events = self.claim_batch().await?;
        let count = events.len();
        for event in events {
            if let Err(error) = self.publish(&event).await {
                self.mark_retry(&event, &error.to_string()).await?;
                tracing::warn!(
                    service = %self.config.service,
                    event_id = %event.id,
                    subject = %event.event_type,
                    attempts = event.attempts,
                    %error,
                    "outbox publish failed"
                );
            } else {
                self.mark_published(event.id).await?;
                metrics::counter!(
                    "outbox_published_total",
                    "service" => self.config.service.clone(),
                    "subject" => event.event_type
                )
                .increment(1);
            }
        }
        self.update_backlog_metric().await?;
        Ok(count)
    }

    async fn claim_batch(&self) -> Result<Vec<ClaimedEvent>, OutboxError> {
        let lease_seconds = self.config.lease_duration.as_secs().max(1) as i64;
        let query = format!(
            "WITH candidates AS (\
                SELECT id FROM {table} \
                WHERE (status IN ('pending','failed') AND available_at <= now() AND attempts < $1) \
                   OR (status = 'processing' AND locked_at < now() - make_interval(secs => $2)) \
                ORDER BY created_at \
                LIMIT $3 \
                FOR UPDATE SKIP LOCKED\
             ) \
             UPDATE {table} AS outbox \
             SET status='processing', attempts=outbox.attempts+1, locked_at=now(), last_error=NULL \
             FROM candidates \
             WHERE outbox.id=candidates.id \
             RETURNING outbox.id,outbox.event_type,outbox.payload,outbox.attempts,outbox.created_at",
            table = self.config.table
        );
        sqlx::query_as::<_, ClaimedEvent>(&query)
            .bind(self.config.max_attempts)
            .bind(lease_seconds)
            .bind(self.config.batch_size)
            .fetch_all(&self.pool)
            .await
            .map_err(OutboxError::from)
    }

    async fn publish(&self, event: &ClaimedEvent) -> Result<(), OutboxError> {
        validate_subject(&event.event_type)?;
        let bytes = serde_json::to_vec(&event.payload)?;
        let mut headers = HeaderMap::new();
        headers.insert("Nats-Msg-Id", event.id.to_string());
        headers.insert("Nexus-Producer", self.config.service.clone());
        headers.insert("Nexus-Outbox-Created-At", event.created_at.to_rfc3339());
        if let Some(trace_context) = event
            .payload
            .get("trace_context")
            .and_then(Value::as_object)
        {
            for (key, value) in trace_context {
                if let Some(value) = value.as_str() {
                    headers.insert(key.as_str(), value);
                }
            }
        }
        let ack = self
            .context
            .publish_with_headers(event.event_type.clone(), headers, bytes.into())
            .await
            .map_err(|error| OutboxError::Publish(error.to_string()))?;
        tokio::time::timeout(Duration::from_secs(5), ack)
            .await
            .map_err(|_| OutboxError::Publish("publish ack timed out".to_string()))?
            .map_err(|error| OutboxError::Publish(error.to_string()))?;
        Ok(())
    }

    async fn mark_published(&self, id: Uuid) -> Result<(), OutboxError> {
        let query = format!(
            "UPDATE {} SET status='published',published_at=now(),locked_at=NULL,last_error=NULL \
             WHERE id=$1 AND status='processing'",
            self.config.table
        );
        sqlx::query(&query).bind(id).execute(&self.pool).await?;
        Ok(())
    }

    async fn mark_retry(&self, event: &ClaimedEvent, error: &str) -> Result<(), OutboxError> {
        let terminal = event.attempts >= self.config.max_attempts;
        let delay_seconds = retry_delay_seconds(event.attempts);
        let query = format!(
            "UPDATE {} SET status=$2,available_at=now()+make_interval(secs => $3),\
             locked_at=NULL,last_error=$4 WHERE id=$1 AND status='processing'",
            self.config.table
        );
        sqlx::query(&query)
            .bind(event.id)
            .bind(if terminal { "failed" } else { "pending" })
            .bind(delay_seconds)
            .bind(truncate_error(error))
            .execute(&self.pool)
            .await?;
        if terminal {
            metrics::counter!("outbox_failed_total", "service" => self.config.service.clone())
                .increment(1);
        }
        Ok(())
    }

    async fn update_backlog_metric(&self) -> Result<(), OutboxError> {
        let query = format!(
            "SELECT count(*) FROM {} WHERE status IN ('pending','processing','failed')",
            self.config.table
        );
        let backlog: i64 = sqlx::query_scalar(&query).fetch_one(&self.pool).await?;
        metrics::gauge!("outbox_backlog", "service" => self.config.service.clone())
            .set(backlog as f64);
        Ok(())
    }
}

fn retry_delay_seconds(attempts: i32) -> i64 {
    let exponent = attempts.saturating_sub(1).min(8) as u32;
    (1_i64 << exponent).min(300)
}

fn truncate_error(error: &str) -> String {
    error.chars().take(2_000).collect()
}

fn validate_table(table: &str) -> Result<(), OutboxError> {
    let segments = table.split('.').collect::<Vec<_>>();
    if !(1..=2).contains(&segments.len())
        || !segments.iter().all(|segment| valid_identifier(segment))
    {
        return Err(OutboxError::InvalidTable(table.to_string()));
    }
    Ok(())
}

fn validate_subject(subject: &str) -> Result<(), OutboxError> {
    if ALL_EVENT_SUBJECTS.contains(&subject) {
        Ok(())
    } else {
        Err(OutboxError::InvalidSubject(subject.to_string()))
    }
}

fn valid_identifier(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
}

fn env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

fn env_i64(key: &str, default: i64) -> i64 {
    std::env::var(key)
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_public_and_schema_qualified_table_names() {
        assert!(validate_table("billing.event_outbox").is_ok());
        assert!(validate_table("event_outbox").is_ok());
        assert!(validate_table("billing.event_outbox;drop").is_err());
        assert!(validate_table("Billing.event_outbox").is_err());
        assert!(validate_table("one.two.three").is_err());
    }

    #[test]
    fn retry_delay_is_bounded() {
        assert_eq!(retry_delay_seconds(1), 1);
        assert_eq!(retry_delay_seconds(5), 16);
        assert_eq!(retry_delay_seconds(100), 256);
    }

    #[test]
    fn rejects_non_canonical_subjects() {
        assert!(validate_subject("usage.recorded").is_ok());
        assert!(validate_subject("events.usage.recorded").is_err());
        assert!(validate_subject("dlq.usage.recorded").is_err());
    }
}
