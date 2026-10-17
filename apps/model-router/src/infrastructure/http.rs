use axum::extract::State;
use axum::middleware::from_fn;
use axum::routing::get;
use axum::Router;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Default)]
pub struct ModelRouterMetrics {
    pub health_checks: AtomicU64,
}

#[derive(Clone)]
pub struct ModelRouterHttpState {
    pub metrics: Arc<ModelRouterMetrics>,
}

pub fn router(state: ModelRouterHttpState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .layer(from_fn(common::observability::http_trace_middleware))
        .with_state(state)
}

async fn health(State(state): State<ModelRouterHttpState>) -> &'static str {
    state.metrics.health_checks.fetch_add(1, Ordering::Relaxed);
    "ok"
}

async fn metrics(State(state): State<ModelRouterHttpState>) -> String {
    format!(
        concat!(
            "# TYPE model_router_health_checks_total counter\n",
            "model_router_health_checks_total {}\n"
        ),
        state.metrics.health_checks.load(Ordering::Relaxed)
    )
}
