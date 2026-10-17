use crate::application::service::BillingApplication;
use crate::domain::errors::BillingError;
use crate::infrastructure::http::BillingMetrics;
use events::jetstream::{
    ack, dead_letter, delivery_attempts, ensure_streams, pull_consumer_config,
    retry_or_dead_letter, DEFAULT_MAX_DELIVERIES, EVENTS_STREAM,
};
use events::outbox::OutboxStore;
use events::{
    BillingPeriodClosed, EventEnvelope, PaymentSucceeded, UsageRecorded,
    EVENT_BILLING_PERIOD_CLOSED, EVENT_PAYMENT_SUCCEEDED, EVENT_USAGE_RECORDED,
};
use futures_util::StreamExt;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn, Instrument};

#[derive(Clone)]
pub struct BillingEventPublisher {
    pub outbox: OutboxStore,
    pub context: async_nats::jetstream::Context,
}

impl BillingEventPublisher {
    pub async fn publish_billing_period_closed(
        &self,
        event: &BillingPeriodClosed,
    ) -> Result<(), BillingError> {
        let envelope = EventEnvelope::new(EVENT_BILLING_PERIOD_CLOSED, event);
        self.outbox
            .enqueue(EVENT_BILLING_PERIOD_CLOSED, &envelope)
            .await
            .map(|_| ())
            .map_err(|error| BillingError::DependencyUnavailable(error.to_string()))
    }
}

pub async fn ensure_jetstream_stream(
    context: &async_nats::jetstream::Context,
) -> Result<(), BillingError> {
    ensure_streams(context)
        .await
        .map_err(|error| BillingError::DependencyUnavailable(error.to_string()))
}

pub async fn run_payment_succeeded_consumer(
    app: Arc<BillingApplication>,
    metrics: Arc<BillingMetrics>,
) -> Result<(), BillingError> {
    const CONSUMER: &str = "billing-payment-consumer";
    let stream = app
        .events
        .context
        .get_stream(EVENTS_STREAM)
        .await
        .map_err(dependency_error)?;
    let consumer = stream
        .create_consumer(
            pull_consumer_config(CONSUMER, EVENT_PAYMENT_SUCCEEDED, 20)
                .map_err(|error| BillingError::DependencyUnavailable(error.to_string()))?,
        )
        .await
        .map_err(dependency_error)?;
    info!(
        subject = EVENT_PAYMENT_SUCCEEDED,
        consumer = CONSUMER,
        "billing JetStream consumer started"
    );

    loop {
        let mut batch = match consumer.batch().max_messages(20).messages().await {
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
                if let Err(error) = dead_letter(
                    &app.events.context,
                    &message,
                    CONSUMER,
                    "processing delivery limit exceeded",
                )
                .await
                {
                    error!(%error, consumer = CONSUMER, "failed to retry DLQ publication");
                } else {
                    metrics.events_dlq.fetch_add(1, Ordering::Relaxed);
                }
                continue;
            }
            metrics.events_received.fetch_add(1, Ordering::Relaxed);
            let span = common::observability::nats_consumer_span(
                message.subject.as_str(),
                CONSUMER,
                message.headers.as_ref(),
            );
            async {
                match EventEnvelope::<PaymentSucceeded>::from_json(&message.payload) {
                Ok(envelope) => match app.process_payment_succeeded(envelope.payload).await {
                    Ok(_) => {
                        if let Err(error) = ack(&message).await {
                            warn!(%error, consumer = CONSUMER, "failed to ACK event");
                        }
                    }
                    Err(processing_error) => {
                        metrics.events_failed.fetch_add(1, Ordering::Relaxed);
                        error!(%processing_error, consumer = CONSUMER, "failed to process event");
                        let will_dead_letter =
                            delivery_attempts(&message) >= DEFAULT_MAX_DELIVERIES;
                        if let Err(error) = retry_or_dead_letter(
                            &app.events.context,
                            &message,
                            CONSUMER,
                            processing_error.to_string(),
                        )
                        .await
                        {
                            error!(%error, consumer = CONSUMER, "failed to NAK/dead-letter event");
                        } else if will_dead_letter {
                            metrics.events_dlq.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                },
                Err(parse_error) => {
                    metrics.events_failed.fetch_add(1, Ordering::Relaxed);
                    warn!(%parse_error, consumer = CONSUMER, "invalid event envelope");
                    if let Err(error) = dead_letter(
                        &app.events.context,
                        &message,
                        CONSUMER,
                        parse_error.to_string(),
                    )
                    .await
                    {
                        error!(%error, consumer = CONSUMER, "failed to dead-letter invalid event");
                    } else {
                        metrics.events_dlq.fetch_add(1, Ordering::Relaxed);
                    }
                }
                }
            }
            .instrument(span)
            .await;
        }
    }
}

pub async fn run_usage_recorded_consumer(
    app: Arc<BillingApplication>,
    metrics: Arc<BillingMetrics>,
) -> Result<(), BillingError> {
    const CONSUMER: &str = "billing-usage-consumer";
    let stream = app
        .events
        .context
        .get_stream(EVENTS_STREAM)
        .await
        .map_err(dependency_error)?;
    let consumer = stream
        .create_consumer(
            pull_consumer_config(CONSUMER, EVENT_USAGE_RECORDED, 50)
                .map_err(|error| BillingError::DependencyUnavailable(error.to_string()))?,
        )
        .await
        .map_err(dependency_error)?;
    info!(
        subject = EVENT_USAGE_RECORDED,
        consumer = CONSUMER,
        "billing JetStream consumer started"
    );

    loop {
        let mut batch = match consumer.batch().max_messages(50).messages().await {
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
                if let Err(error) = dead_letter(
                    &app.events.context,
                    &message,
                    CONSUMER,
                    "processing delivery limit exceeded",
                )
                .await
                {
                    error!(%error, consumer = CONSUMER, "failed to retry DLQ publication");
                } else {
                    metrics.events_dlq.fetch_add(1, Ordering::Relaxed);
                }
                continue;
            }
            metrics.events_received.fetch_add(1, Ordering::Relaxed);
            let span = common::observability::nats_consumer_span(
                message.subject.as_str(),
                CONSUMER,
                message.headers.as_ref(),
            );
            async {
                match EventEnvelope::<UsageRecorded>::from_json(&message.payload) {
                Ok(envelope) => match app.process_usage_recorded(envelope.payload).await {
                    Ok(()) => {
                        if let Err(error) = ack(&message).await {
                            warn!(%error, consumer = CONSUMER, "failed to ACK event");
                        }
                    }
                    Err(processing_error) => {
                        metrics.events_failed.fetch_add(1, Ordering::Relaxed);
                        error!(%processing_error, consumer = CONSUMER, "failed to process event");
                        let will_dead_letter =
                            delivery_attempts(&message) >= DEFAULT_MAX_DELIVERIES;
                        if let Err(error) = retry_or_dead_letter(
                            &app.events.context,
                            &message,
                            CONSUMER,
                            processing_error.to_string(),
                        )
                        .await
                        {
                            error!(%error, consumer = CONSUMER, "failed to NAK/dead-letter event");
                        } else if will_dead_letter {
                            metrics.events_dlq.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                },
                Err(parse_error) => {
                    metrics.events_failed.fetch_add(1, Ordering::Relaxed);
                    warn!(%parse_error, consumer = CONSUMER, "invalid event envelope");
                    if let Err(error) = dead_letter(
                        &app.events.context,
                        &message,
                        CONSUMER,
                        parse_error.to_string(),
                    )
                    .await
                    {
                        error!(%error, consumer = CONSUMER, "failed to dead-letter invalid event");
                    } else {
                        metrics.events_dlq.fetch_add(1, Ordering::Relaxed);
                    }
                }
                }
            }
            .instrument(span)
            .await;
        }
    }
}

pub async fn run_reservation_expiration_job(app: Arc<BillingApplication>) {
    let mut interval = tokio::time::interval(Duration::from_secs(60));
    loop {
        interval.tick().await;
        match app.expire_overdue_reservations().await {
            Ok(count) if count > 0 => info!(count, "expired overdue reservations"),
            Ok(_) => {}
            Err(error) => error!(%error, "failed to expire overdue reservations"),
        }
    }
}

fn dependency_error(error: impl std::fmt::Display) -> BillingError {
    BillingError::DependencyUnavailable(error.to_string())
}
