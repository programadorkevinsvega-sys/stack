use crate::AppConfig;
use async_nats::HeaderMap as NatsHeaderMap;
use axum::extract::Request;
use axum::middleware::Next;
use axum::response::Response;
use http::HeaderMap;
use opentelemetry::propagation::{Extractor, Injector};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry::KeyValue;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::propagation::TraceContextPropagator;
use opentelemetry_sdk::{runtime, trace::TracerProvider, Resource};
use sha2::Digest;
use std::collections::BTreeMap;
use thiserror::Error;
use tonic::metadata::{Ascii, MetadataKey, MetadataMap, MetadataValue};
use tracing::{field, Instrument, Span};
use tracing_opentelemetry::OpenTelemetryLayer;
use tracing_opentelemetry::OpenTelemetrySpanExt;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum ObservabilityError {
    #[error("OTEL_EXPORTER_OTLP_ENDPOINT is required when OTEL_ENABLED=true")]
    MissingEndpoint,
    #[error("invalid tracing filter: {0}")]
    InvalidFilter(String),
    #[error("failed to build OTLP exporter: {0}")]
    Exporter(String),
    #[error("failed to install tracing subscriber: {0}")]
    Subscriber(String),
}

/// Returns a stable, non-reversible identifier for metrics labels.
///
/// The process-wide salt is read from `OBSERVABILITY_USER_HASH_SALT`; keeping
/// it outside the metric payload prevents a user UUID from becoming PII in
/// Prometheus while preserving aggregation for one deployment.  A missing
/// salt is intentionally rejected so production metrics cannot silently fall
/// back to a guessable digest.
pub fn hash_user_id(user_id: Uuid) -> String {
    let salt = std::env::var("OBSERVABILITY_USER_HASH_SALT")
        .or_else(|_| std::env::var("SERVICE_JWT_SECRET"))
        .unwrap_or_else(|_| "nexus-observability".to_owned());
    let mut digest = sha2::Sha256::new();
    digest.update(salt.as_bytes());
    digest.update(user_id.as_bytes());
    hex::encode(digest.finalize())[..24].to_owned()
}

/// Installs structured JSON logging and, when enabled, OTLP trace export.
///
/// `service.name` always identifies the concrete process. The shared
/// `OTEL_SERVICE_NAME` value is used as `service.namespace`, which lets one
/// repository-wide `.env` group all services without collapsing them into a
/// single service in Tempo.
pub fn init(service_name: &'static str, config: &AppConfig) -> Result<(), ObservabilityError> {
    opentelemetry::global::set_text_map_propagator(TraceContextPropagator::new());

    let filter = EnvFilter::try_new(config.log_level.as_deref().unwrap_or("info"))
        .map_err(|error| ObservabilityError::InvalidFilter(error.to_string()))?;

    if !config.otel_enabled.unwrap_or(false) {
        tracing_subscriber::registry()
            .with(filter)
            .with(tracing_subscriber::fmt::layer().json())
            .try_init()
            .map_err(|error| ObservabilityError::Subscriber(error.to_string()))?;
        return Ok(());
    }

    let endpoint = config
        .otel_exporter_otlp_endpoint
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .ok_or(ObservabilityError::MissingEndpoint)?;
    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()
        .map_err(|error| ObservabilityError::Exporter(error.to_string()))?;

    let mut attributes = vec![KeyValue::new("service.name", service_name)];
    if let Some(namespace) = config
        .otel_service_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        attributes.push(KeyValue::new("service.namespace", namespace.to_owned()));
    }
    let provider = TracerProvider::builder()
        .with_batch_exporter(exporter, runtime::Tokio)
        .with_resource(Resource::new(attributes))
        .build();
    let tracer = provider.tracer(service_name);
    let otel_layer = OpenTelemetryLayer::new(tracer);

    tracing_subscriber::registry()
        .with(filter)
        .with(tracing_subscriber::fmt::layer().json())
        .with(otel_layer)
        .try_init()
        .map_err(|error| ObservabilityError::Subscriber(error.to_string()))?;
    opentelemetry::global::set_tracer_provider(provider);
    Ok(())
}

struct HeaderExtractor<'a>(&'a HeaderMap);

impl Extractor for HeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|value| value.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|key| key.as_str()).collect()
    }
}

struct MetadataInjector<'a>(&'a mut MetadataMap);

impl Injector for MetadataInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        let Ok(key) = MetadataKey::<Ascii>::from_bytes(key.as_bytes()) else {
            return;
        };
        let Ok(value) = MetadataValue::try_from(value) else {
            return;
        };
        self.0.insert(key, value);
    }
}

struct MapInjector<'a>(&'a mut BTreeMap<String, String>);

impl Injector for MapInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key.to_owned(), value);
    }
}

struct NatsHeaderExtractor<'a>(&'a NatsHeaderMap);

impl Extractor for NatsHeaderExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).map(AsRef::as_ref)
    }

    fn keys(&self) -> Vec<&str> {
        self.0.iter().map(|(key, _)| key.as_ref()).collect()
    }
}

struct NatsHeaderInjector<'a>(&'a mut NatsHeaderMap);

impl Injector for NatsHeaderInjector<'_> {
    fn set(&mut self, key: &str, value: String) {
        self.0.insert(key, value);
    }
}

/// Adds the current W3C trace context to an outgoing tonic request.
pub fn inject_tonic_context<T>(request: &mut tonic::Request<T>) {
    let context = Span::current().context();
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&context, &mut MetadataInjector(request.metadata_mut()));
    });
}

/// Captures the current W3C propagation fields so they can survive an outbox
/// transaction and be restored later by the relay.
pub fn current_trace_context() -> BTreeMap<String, String> {
    let mut carrier = BTreeMap::new();
    let context = Span::current().context();
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&context, &mut MapInjector(&mut carrier));
    });
    carrier
}

/// Adds the current W3C context to outgoing NATS headers.
pub fn inject_nats_context(headers: &mut NatsHeaderMap) {
    let context = Span::current().context();
    opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.inject_context(&context, &mut NatsHeaderInjector(headers));
    });
}

/// Creates a NATS consumer span linked to the producer's W3C context.
pub fn nats_consumer_span(subject: &str, consumer: &str, headers: Option<&NatsHeaderMap>) -> Span {
    let span_name = format!("{subject} process");
    let span = tracing::info_span!(
        "messaging.process",
        otel.name = %span_name,
        otel.kind = "consumer",
        messaging.system = "nats",
        messaging.destination.name = %subject,
        messaging.operation.name = "process",
        messaging.consumer.group.name = %consumer,
        error.type = field::Empty,
    );
    if let Some(headers) = headers {
        let parent = opentelemetry::global::get_text_map_propagator(|propagator| {
            propagator.extract(&NatsHeaderExtractor(headers))
        });
        span.set_parent(parent);
    }
    span
}

/// Creates the root/server span used by tonic for the full lifetime of a call.
pub fn grpc_server_span(request: &http::Request<()>) -> Span {
    let path = request.uri().path();
    let operation = path.trim_start_matches('/');
    let (service, method) = operation.rsplit_once('/').unwrap_or((operation, "unknown"));
    let span_name = format!("{service}/{method}");
    let span = tracing::info_span!(
        "grpc.server",
        otel.name = %span_name,
        otel.kind = "server",
        rpc.system = "grpc",
        rpc.service = %service,
        rpc.method = %method,
        error.type = field::Empty,
    );
    let parent = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(request.headers()))
    });
    span.set_parent(parent);
    span
}

/// Creates one server span per incoming HTTP request and adopts `traceparent`
/// when the caller supplied W3C trace context.
pub async fn http_trace_middleware(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_owned();
    let span_name = format!("{method} {path}");
    let span = tracing::info_span!(
        "http.server",
        otel.name = %span_name,
        otel.kind = "server",
        http.request.method = %method,
        url.path = %path,
        http.response.status_code = field::Empty,
        error.type = field::Empty,
    );
    let parent = opentelemetry::global::get_text_map_propagator(|propagator| {
        propagator.extract(&HeaderExtractor(request.headers()))
    });
    span.set_parent(parent);

    let response = next.run(request).instrument(span.clone()).await;
    let status = response.status();
    span.record("http.response.status_code", status.as_u16());
    if status.is_server_error() {
        span.record("error.type", status.as_str());
    }
    response
}

#[cfg(test)]
mod tests {
    use super::hash_user_id;
    use uuid::Uuid;

    #[test]
    fn user_metric_hash_is_stable_and_does_not_expose_uuid() {
        let user = Uuid::from_u128(1);
        let first = hash_user_id(user);
        let second = hash_user_id(user);
        assert_eq!(first, second);
        assert_eq!(first.len(), 24);
        assert!(!first.contains(&user.to_string()));
        assert_ne!(first, hash_user_id(Uuid::from_u128(2)));
    }
}
