use crate::application::service::BillingPeriodProcessor;
use crate::domain::errors::InvoiceError;
use crate::infrastructure::http::InvoiceMetrics;
use events::jetstream::{
    ack, dead_letter, delivery_attempts, ensure_streams, pull_consumer_config,
    retry_or_dead_letter, DEFAULT_MAX_DELIVERIES, EVENTS_STREAM,
};
use events::{BillingPeriodClosed, EventEnvelope, EVENT_BILLING_PERIOD_CLOSED};
use futures_util::StreamExt;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn, Instrument};

#[derive(Clone)]
pub struct InvoiceEventBus {
    pub context: async_nats::jetstream::Context,
}

pub async fn ensure_jetstream_stream(
    context: &async_nats::jetstream::Context,
) -> Result<(), InvoiceError> {
    ensure_streams(context)
        .await
        .map_err(|error| InvoiceError::DependencyUnavailable(error.to_string()))
}

pub async fn run_billing_period_closed_consumer<T>(
    app: Arc<T>,
    bus: InvoiceEventBus,
    metrics: Arc<InvoiceMetrics>,
) -> Result<(), InvoiceError>
where
    T: BillingPeriodProcessor + 'static,
{
    const CONSUMER: &str = "invoice-service";
    let stream = bus
        .context
        .get_stream(EVENTS_STREAM)
        .await
        .map_err(dependency_error)?;
    let consumer = stream
        .create_consumer(
            pull_consumer_config(CONSUMER, EVENT_BILLING_PERIOD_CLOSED, 20)
                .map_err(|error| InvoiceError::DependencyUnavailable(error.to_string()))?,
        )
        .await
        .map_err(dependency_error)?;
    info!(
        subject = EVENT_BILLING_PERIOD_CLOSED,
        consumer = CONSUMER,
        "invoice JetStream consumer started"
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
                            .billing_period_closed_dlq
                            .fetch_add(1, Ordering::Relaxed);
                    }
                    Err(error) => {
                        error!(%error, consumer = CONSUMER, "failed to retry DLQ publication");
                    }
                }
                continue;
            }
            metrics
                .billing_period_closed_received
                .fetch_add(1, Ordering::Relaxed);
            let span = common::observability::nats_consumer_span(
                message.subject.as_str(),
                CONSUMER,
                message.headers.as_ref(),
            );
            async {
                match EventEnvelope::<BillingPeriodClosed>::from_json(&message.payload) {
                Ok(envelope) => match app.process_billing_period_closed(envelope.payload).await {
                    Ok(document) => {
                        metrics.invoices_generated.fetch_add(1, Ordering::Relaxed);
                        info!(
                            invoice_id = %document.invoice.id,
                            invoice_number = %document.invoice.invoice_number,
                            "invoice generated"
                        );
                        if let Err(error) = ack(&message).await {
                            warn!(%error, consumer = CONSUMER, "failed to ACK event");
                        }
                    }
                    Err(processing_error) => {
                        metrics
                            .billing_period_closed_failed
                            .fetch_add(1, Ordering::Relaxed);
                        error!(%processing_error, consumer = CONSUMER, "failed to process event");
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
                                    .billing_period_closed_dlq
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
                    metrics
                        .billing_period_closed_failed
                        .fetch_add(1, Ordering::Relaxed);
                    warn!(%parse_error, consumer = CONSUMER, "invalid event envelope");
                    match dead_letter(&bus.context, &message, CONSUMER, parse_error.to_string())
                        .await
                    {
                        Ok(()) => {
                            metrics
                                .billing_period_closed_dlq
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

fn dependency_error(error: impl std::fmt::Display) -> InvoiceError {
    InvoiceError::DependencyUnavailable(error.to_string())
}
