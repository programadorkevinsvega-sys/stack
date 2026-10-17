use axum::extract::State;
use axum::middleware::from_fn;
use axum::routing::get;
use axum::Router;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub struct UsageMetrics {
    pub health_checks: AtomicU64,
    pub usage_events_received: AtomicU64,
    pub usage_events_persisted: AtomicU64,
    pub usage_events_duplicate: AtomicU64,
    pub usage_events_failed: AtomicU64,
    pub usage_events_dlq_published: AtomicU64,
}

#[derive(Clone)]
pub struct UsageHttpState {
    pub metrics: Arc<UsageMetrics>,
}

pub fn router(state: UsageHttpState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .layer(from_fn(common::observability::http_trace_middleware))
        .with_state(state)
}

async fn health(State(state): State<UsageHttpState>) -> &'static str {
    state.metrics.health_checks.fetch_add(1, Ordering::Relaxed);
    "ok"
}

async fn metrics(State(state): State<UsageHttpState>) -> String {
    format!(
        concat!(
            "# TYPE usage_health_checks_total counter\n",
            "usage_health_checks_total {}\n",
            "# TYPE usage_events_received_total counter\n",
            "usage_events_received_total {}\n",
            "# TYPE usage_events_persisted_total counter\n",
            "usage_events_persisted_total {}\n",
            "# TYPE usage_events_duplicate_total counter\n",
            "usage_events_duplicate_total {}\n",
            "# TYPE usage_events_failed_total counter\n",
            "usage_events_failed_total {}\n",
            "# TYPE usage_events_dlq_published_total counter\n",
            "usage_events_dlq_published_total {}\n",
            "# TYPE events_dlq_total counter\n",
            "events_dlq_total{{service=\"usage-service\"}} {}\n"
        ),
        state.metrics.health_checks.load(Ordering::Relaxed),
        state.metrics.usage_events_received.load(Ordering::Relaxed),
        state.metrics.usage_events_persisted.load(Ordering::Relaxed),
        state.metrics.usage_events_duplicate.load(Ordering::Relaxed),
        state.metrics.usage_events_failed.load(Ordering::Relaxed),
        state
            .metrics
            .usage_events_dlq_published
            .load(Ordering::Relaxed),
        state
            .metrics
            .usage_events_dlq_published
            .load(Ordering::Relaxed)
    )
}
