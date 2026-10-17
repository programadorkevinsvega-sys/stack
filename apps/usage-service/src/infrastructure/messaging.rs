use crate::application::service::UsageApplication;
use crate::domain::errors::UsageError;
use crate::infrastructure::http::UsageMetrics;
use events::jetstream::{
    ack, dead_letter, delivery_attempts, ensure_streams, pull_consumer_config,
    retry_or_dead_letter, DEFAULT_MAX_DELIVERIES, EVENTS_STREAM,
};
use events::{EventEnvelope, UsageRecorded, EVENT_USAGE_RECORDED};
use futures_util::StreamExt;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn, Instrument};

#[derive(Clone)]
pub struct UsageEventBus {
    pub context: async_nats::jetstream::Context,
}

pub async fn ensure_jetstream_stream(
    context: &async_nats::jetstream::Context,
) -> Result<(), UsageError> {
    ensure_streams(context)
        .await
        .map_err(|error| UsageError::DependencyUnavailable(error.to_string()))
}

pub async fn run_usage_recorded_consumer(
    app: Arc<UsageApplication>,
    bus: UsageEventBus,
    metrics: Arc<UsageMetrics>,
) -> Result<(), UsageError> {
    const CONSUMER: &str = "usage-service";
    let stream = bus
        .context
        .get_stream(EVENTS_STREAM)
        .await
        .map_err(dependency_error)?;
    let consumer = stream
        .create_consumer(
            pull_consumer_config(CONSUMER, EVENT_USAGE_RECORDED, 100)
                .map_err(|error| UsageError::DependencyUnavailable(error.to_string()))?,
        )
        .await
        .map_err(dependency_error)?;
    info!(
        subject = EVENT_USAGE_RECORDED,
        consumer = CONSUMER,
        "usage JetStream consumer started"
    );

    loop {
        let mut batch = match consumer.batch().max_messages(100).messages().await {
            Ok(batch) => batch,
            Err(error) => {
                warn!(%error, consumer = CONSUMER, "failed to fetch JetStream batch");
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            }
        };
        while let Some(message) = batch.next().await {
            let message = match message {
                Ok(message) => message,
                Err(error) => {
                    warn!(%error, consumer = CONSUMER, "JetStream message error");
                    continue;
                }
            };
            if delivery_attempts(&message) > DEFAULT_MAX_DELIVERIES {
                match dead_letter(
                    &bus.context,
                    &message,
                    CONSUMER,
                    "processing delivery limit exceeded",
                )
                .await
                {
                    Ok(()) => {
                        metrics
                            .usage_events_dlq_published
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    Err(error) => {
                        error!(%error, consumer = CONSUMER, "failed to retry DLQ publication");
                    }
                }
                continue;
            }
            metrics
                .usage_events_received
                .fetch_add(1, Ordering::Relaxed);
            let span = common::observability::nats_consumer_span(
                message.subject.as_str(),
                CONSUMER,
                message.headers.as_ref(),
            );
            async {
                match EventEnvelope::<UsageRecorded>::from_json(&message.payload) {
                Ok(envelope) => match app
                    .record_usage_event(envelope.payload, envelope.occurred_at)
                    .await
                {
                    Ok(true) => {
                        metrics
                            .usage_events_persisted
                            .fetch_add(1, Ordering::Relaxed);
                        if let Err(error) = ack(&message).await {
                            warn!(%error, consumer = CONSUMER, "failed to ACK event");
                        }
                    }
                    Ok(false) => {
                        metrics
                            .usage_events_duplicate
                            .fetch_add(1, Ordering::Relaxed);
                        info!(consumer = CONSUMER, "duplicate usage event ignored");
                        if let Err(error) = ack(&message).await {
                            warn!(%error, consumer = CONSUMER, "failed to ACK duplicate event");
                        }
                    }
                    Err(processing_error) => {
                        metrics.usage_events_failed.fetch_add(1, Ordering::Relaxed);
                        error!(%processing_error, consumer = CONSUMER, "failed to persist event");
                        let will_dead_letter =
                            delivery_attempts(&message) >= DEFAULT_MAX_DELIVERIES;
                        match retry_or_dead_letter(
                            &bus.context,
                            &message,
                            CONSUMER,
                            processing_error.to_string(),
                        )
                        .await
                        {
                            Ok(()) if will_dead_letter => {
                                metrics
                                    .usage_events_dlq_published
                                    .fetch_add(1, Ordering::Relaxed);
                            }
                            Ok(()) => {}
                            Err(error) => {
                                error!(%error, consumer = CONSUMER, "failed to NAK/dead-letter event");
                            }
                        }
                    }
                },
                Err(parse_error) => {
                    metrics.usage_events_failed.fetch_add(1, Ordering::Relaxed);
                    warn!(%parse_error, consumer = CONSUMER, "invalid event envelope");
                    match dead_letter(&bus.context, &message, CONSUMER, parse_error.to_string())
                        .await
                    {
                        Ok(()) => {
                            metrics
                                .usage_events_dlq_published
                                .fetch_add(1, Ordering::Relaxed);
                        }
                        Err(error) => {
                            error!(%error, consumer = CONSUMER, "failed to dead-letter invalid event");
                        }
                    }
                }
                }
            }
            .instrument(span)
            .await;
        }
    }
}

pub async fn run_partition_management_job(app: Arc<UsageApplication>) {
    let mut interval = tokio::time::interval(Duration::from_secs(6 * 60 * 60));
    loop {
        interval.tick().await;
        match app.create_next_month_partition().await {
            Ok(partition) => info!(%partition, "usage partition verified"),
            Err(error) => error!(%error, "failed to verify usage partition"),
        }
    }
}

pub async fn run_materialized_view_refresh_job(app: Arc<UsageApplication>) {
    let mut interval = tokio::time::interval(Duration::from_secs(5 * 60));
    loop {
        interval.tick().await;
        match app.refresh_materialized_views().await {
            Ok(()) => info!("usage materialized views refreshed"),
            Err(error) => error!(%error, "failed to refresh usage materialized views"),
        }
    }
}

fn dependency_error(error: impl std::fmt::Display) -> UsageError {
    UsageError::DependencyUnavailable(error.to_string())
}
