use axum::extract::State;
use axum::middleware::from_fn;
use axum::routing::get;
use axum::Router;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub struct BillingMetrics {
    pub health_checks: AtomicU64,
    pub events_received: AtomicU64,
    pub events_failed: AtomicU64,
    pub events_dlq: AtomicU64,
}

#[derive(Clone)]
pub struct BillingHttpState {
    pub metrics: Arc<BillingMetrics>,
}

pub fn router(state: BillingHttpState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .layer(from_fn(common::observability::http_trace_middleware))
        .with_state(state)
}

async fn health(State(state): State<BillingHttpState>) -> &'static str {
    state.metrics.health_checks.fetch_add(1, Ordering::Relaxed);
    "ok"
}

async fn metrics(State(state): State<BillingHttpState>) -> String {
    format!(
        concat!(
            "# TYPE billing_health_checks_total counter\n",
            "billing_health_checks_total {}\n",
            "# TYPE billing_events_received_total counter\n",
            "billing_events_received_total {}\n",
            "# TYPE billing_events_failed_total counter\n",
            "billing_events_failed_total {}\n",
            "# TYPE events_dlq_total counter\n",
            "events_dlq_total{{service=\"billing-service\"}} {}\n"
        ),
        state.metrics.health_checks.load(Ordering::Relaxed),
        state.metrics.events_received.load(Ordering::Relaxed),
        state.metrics.events_failed.load(Ordering::Relaxed),
        state.metrics.events_dlq.load(Ordering::Relaxed)
    )
}
