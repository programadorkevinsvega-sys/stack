use axum::extract::State;
use axum::middleware::from_fn;
use axum::routing::get;
use axum::Router;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub struct InvoiceMetrics {
    pub health_checks: AtomicU64,
    pub billing_period_closed_received: AtomicU64,
    pub billing_period_closed_failed: AtomicU64,
    pub billing_period_closed_dlq: AtomicU64,
    pub invoices_generated: AtomicU64,
}

#[derive(Clone)]
pub struct InvoiceHttpState {
    pub metrics: Arc<InvoiceMetrics>,
}

pub fn router(state: InvoiceHttpState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .layer(from_fn(common::observability::http_trace_middleware))
        .with_state(state)
}

async fn health(State(state): State<InvoiceHttpState>) -> &'static str {
    state.metrics.health_checks.fetch_add(1, Ordering::Relaxed);
    "ok"
}

async fn metrics(State(state): State<InvoiceHttpState>) -> String {
    format!(
        concat!(
            "# TYPE invoice_health_checks_total counter\n",
            "invoice_health_checks_total {}\n",
            "# TYPE invoice_billing_period_closed_received_total counter\n",
            "invoice_billing_period_closed_received_total {}\n",
            "# TYPE invoice_billing_period_closed_failed_total counter\n",
            "invoice_billing_period_closed_failed_total {}\n",
            "# TYPE invoice_billing_period_closed_dlq_total counter\n",
            "invoice_billing_period_closed_dlq_total {}\n",
            "# TYPE invoices_generated_total counter\n",
            "invoices_generated_total {}\n",
            "# TYPE events_dlq_total counter\n",
            "events_dlq_total{{service=\"invoice-service\"}} {}\n"
        ),
        state.metrics.health_checks.load(Ordering::Relaxed),
        state
            .metrics
            .billing_period_closed_received
            .load(Ordering::Relaxed),
        state
            .metrics
            .billing_period_closed_failed
            .load(Ordering::Relaxed),
        state
            .metrics
            .billing_period_closed_dlq
            .load(Ordering::Relaxed),
        state.metrics.invoices_generated.load(Ordering::Relaxed),
        state
            .metrics
            .billing_period_closed_dlq
            .load(Ordering::Relaxed)
    )
}
