use crate::ALL_EVENT_SUBJECTS;
use async_nats::jetstream;
use async_nats::jetstream::consumer::AckPolicy;
use async_nats::jetstream::message::AckKind;
use async_nats::jetstream::stream::{RetentionPolicy, StorageType};
use async_nats::HeaderMap;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

pub const EVENTS_STREAM: &str = "EVENTS";
pub const DLQ_STREAM: &str = "EVENTS_DLQ";
pub const DEFAULT_MAX_DELIVERIES: i64 = 5;
const DLQ_PUBLISH_DELIVERIES: i64 = 5;

#[derive(Debug, Error)]
pub enum JetStreamError {
    #[error("JetStream operation failed: {0}")]
    JetStream(String),
    #[error("invalid consumer name: {0}")]
    InvalidConsumerName(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeadLetter {
    pub id: uuid::Uuid,
    pub consumer: String,
    pub original_subject: String,
    pub delivery_attempts: i64,
    pub error: String,
    pub payload: serde_json::Value,
    pub failed_at: DateTime<Utc>,
}

pub async fn ensure_streams(context: &jetstream::Context) -> Result<(), JetStreamError> {
    context
        .create_or_update_stream(jetstream::stream::Config {
            name: EVENTS_STREAM.to_string(),
            description: Some("Canonical Nexus domain events".to_string()),
            subjects: ALL_EVENT_SUBJECTS
                .iter()
                .map(|subject| (*subject).to_string())
                .collect(),
            retention: RetentionPolicy::Limits,
            storage: StorageType::File,
            max_messages: 1_000_000,
            max_bytes: 1024 * 1024 * 1024,
            max_age: Duration::from_secs(30 * 24 * 60 * 60),
            duplicate_window: Duration::from_secs(24 * 60 * 60),
            ..Default::default()
        })
        .await
        .map_err(|error| JetStreamError::JetStream(error.to_string()))?;
    tracing::info!(
        dependency = "nats_jetstream",
        stream = EVENTS_STREAM,
        storage = "file",
        retention = "limits",
        max_messages = 1_000_000,
        "JetStream stream ready"
    );

    context
        .create_or_update_stream(jetstream::stream::Config {
            name: DLQ_STREAM.to_string(),
            description: Some("Dead-letter events requiring operator action".to_string()),
            subjects: vec!["dlq.>".to_string()],
            retention: RetentionPolicy::Limits,
            storage: StorageType::File,
            max_messages: 100_000,
            max_bytes: 512 * 1024 * 1024,
            max_age: Duration::from_secs(90 * 24 * 60 * 60),
            duplicate_window: Duration::from_secs(24 * 60 * 60),
            ..Default::default()
        })
        .await
        .map_err(|error| JetStreamError::JetStream(error.to_string()))?;
    tracing::info!(
        dependency = "nats_jetstream",
        stream = DLQ_STREAM,
        subject = "dlq.>",
        storage = "file",
        retention = "limits",
        max_messages = 100_000,
        "JetStream DLQ stream ready"
    );

    Ok(())
}

pub fn pull_consumer_config(
    durable_name: &str,
    filter_subject: &str,
    max_ack_pending: i64,
) -> Result<jetstream::consumer::pull::Config, JetStreamError> {
    if !valid_token(durable_name) {
        return Err(JetStreamError::InvalidConsumerName(
            durable_name.to_string(),
        ));
    }
    Ok(jetstream::consumer::pull::Config {
        durable_name: Some(durable_name.to_string()),
        name: Some(durable_name.to_string()),
        description: Some(format!("Durable consumer for {filter_subject}")),
        filter_subject: filter_subject.to_string(),
        ack_policy: AckPolicy::Explicit,
        ack_wait: Duration::from_secs(30),
        // The handler processes at most DEFAULT_MAX_DELIVERIES times. The
        // remaining server deliveries are reserved for retrying a failed DLQ
        // publication, so a temporary DLQ outage cannot silently drop poison
        // messages.
        max_deliver: DEFAULT_MAX_DELIVERIES + DLQ_PUBLISH_DELIVERIES,
        max_ack_pending,
        backoff: vec![
            Duration::from_secs(1),
            Duration::from_secs(5),
            Duration::from_secs(30),
            Duration::from_secs(120),
            Duration::from_secs(300),
        ],
        ..Default::default()
    })
}

pub fn delivery_attempts(message: &jetstream::Message) -> i64 {
    message.info().map(|info| info.delivered).unwrap_or(1)
}

pub async fn ack(message: &jetstream::Message) -> Result<(), JetStreamError> {
    message
        .double_ack()
        .await
        .map_err(|error| JetStreamError::JetStream(error.to_string()))?;
    tracing::debug!(
        dependency = "nats_jetstream",
        subject = %message.subject,
        "message ACK confirmed"
    );
    Ok(())
}

pub async fn retry(message: &jetstream::Message) -> Result<(), JetStreamError> {
    message
        .ack_with(AckKind::Nak(None))
        .await
        .map_err(|error| JetStreamError::JetStream(error.to_string()))?;
    tracing::warn!(
        dependency = "nats_jetstream",
        subject = %message.subject,
        delivery_attempts = delivery_attempts(message),
        "message scheduled for retry"
    );
    Ok(())
}

pub async fn dead_letter(
    context: &jetstream::Context,
    message: &jetstream::Message,
    consumer: &str,
    error: impl Into<String>,
) -> Result<(), JetStreamError> {
    if !valid_token(consumer) {
        return Err(JetStreamError::InvalidConsumerName(consumer.to_string()));
    }
    let attempts = delivery_attempts(message);
    let payload = serde_json::from_slice(&message.payload)
        .unwrap_or_else(|_| serde_json::Value::String(hex_payload(&message.payload)));
    let dead_letter = DeadLetter {
        id: payload
            .get("event_id")
            .and_then(serde_json::Value::as_str)
            .and_then(|value| uuid::Uuid::parse_str(value).ok())
            .unwrap_or_else(uuid::Uuid::now_v7),
        consumer: consumer.to_string(),
        original_subject: message.subject.to_string(),
        delivery_attempts: attempts,
        error: error.into(),
        payload,
        failed_at: Utc::now(),
    };
    let bytes = serde_json::to_vec(&dead_letter)
        .map_err(|error| JetStreamError::JetStream(error.to_string()))?;
    let subject = format!(
        "dlq.{}.{}",
        consumer.replace('-', "_"),
        message.subject.as_str()
    );
    let mut headers = HeaderMap::new();
    let message_id = message
        .info()
        .map(|info| format!("dlq:{consumer}:{}:{}", info.stream, info.stream_sequence))
        .unwrap_or_else(|_| format!("dlq:{consumer}:{}", dead_letter.id));
    headers.insert("Nats-Msg-Id", message_id);
    common::observability::inject_nats_context(&mut headers);
    let publish_ack = context
        .publish_with_headers(subject, headers, bytes.into())
        .await
        .map_err(|error| JetStreamError::JetStream(error.to_string()))?;
    publish_ack
        .await
        .map_err(|error| JetStreamError::JetStream(error.to_string()))?;
    message
        .double_ack_with(AckKind::Term)
        .await
        .map_err(|error| JetStreamError::JetStream(error.to_string()))?;
    metrics::counter!("events_dlq_total", "consumer" => consumer.to_string()).increment(1);
    tracing::error!(
        dependency = "nats_jetstream",
        stream = DLQ_STREAM,
        consumer,
        original_subject = %message.subject,
        delivery_attempts = attempts,
        dead_letter_id = %dead_letter.id,
        "message moved to DLQ and original delivery terminated"
    );
    Ok(())
}

pub async fn retry_or_dead_letter(
    context: &jetstream::Context,
    message: &jetstream::Message,
    consumer: &str,
    error: impl Into<String>,
) -> Result<(), JetStreamError> {
    let error = error.into();
    if delivery_attempts(message) >= DEFAULT_MAX_DELIVERIES {
        dead_letter(context, message, consumer, error).await
    } else {
        retry(message).await
    }
}

fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

fn hex_payload(payload: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut encoded = String::with_capacity(payload.len() * 2);
    for byte in payload {
        encoded.push(HEX[(byte >> 4) as usize] as char);
        encoded.push(HEX[(byte & 0x0f) as usize] as char);
    }
    encoded
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consumer_config_is_durable_and_filtered() {
        let config = pull_consumer_config("billing-usage", "usage.recorded", 20).unwrap();
        assert_eq!(config.durable_name.as_deref(), Some("billing-usage"));
        assert_eq!(config.filter_subject, "usage.recorded");
        assert_eq!(config.ack_policy, AckPolicy::Explicit);
        assert_eq!(
            config.max_deliver,
            DEFAULT_MAX_DELIVERIES + DLQ_PUBLISH_DELIVERIES
        );
    }

    #[test]
    fn consumer_name_rejects_subject_separators() {
        assert!(pull_consumer_config("billing.usage", "usage.recorded", 20).is_err());
    }
}
