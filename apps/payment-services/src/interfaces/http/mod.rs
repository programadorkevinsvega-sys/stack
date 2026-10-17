use crate::application::commands::HandlePayPalWebhook;
use crate::application::ports::PayPalWebhookHeaders;
use axum::body::Bytes;
use axum::extract::Request;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::{get, post};
use axum::Router;
use common::ReloadableRedisPool;
use serde_json::Value;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::timeout::TimeoutLayer;

#[derive(Clone)]
pub struct PaymentHttpState {
    pub paypal_webhook: Arc<HandlePayPalWebhook>,
    pub redis: ReloadableRedisPool,
    pub max_requests_per_minute: usize,
    pub metrics: Arc<PaymentHttpMetrics>,
}

#[derive(Default)]
pub struct PaymentHttpMetrics {
    pub webhook_received: AtomicU64,
    pub webhook_success: AtomicU64,
    pub webhook_failed: AtomicU64,
    pub webhook_rate_limited: AtomicU64,
}

pub fn router(state: PaymentHttpState, max_body_bytes: usize, timeout: Duration) -> Router {
    Router::new()
        .route("/webhooks/paypal", post(paypal_webhook))
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .layer(middleware::from_fn_with_state(
            state.clone(),
            enforce_rate_limit,
        ))
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            timeout,
        ))
        .layer(RequestBodyLimitLayer::new(max_body_bytes))
        .layer(middleware::from_fn(
            common::observability::http_trace_middleware,
        ))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn metrics(State(state): State<PaymentHttpState>) -> String {
    format!(
        concat!(
            "# TYPE payment_webhook_received_total counter\n",
            "payment_webhook_received_total {}\n",
            "# TYPE payment_webhook_success_total counter\n",
            "payment_webhook_success_total {}\n",
            "# TYPE payment_webhook_failed_total counter\n",
            "payment_webhook_failed_total {}\n",
            "# TYPE payment_webhook_rate_limited_total counter\n",
            "payment_webhook_rate_limited_total {}\n"
        ),
        state.metrics.webhook_received.load(Ordering::Relaxed),
        state.metrics.webhook_success.load(Ordering::Relaxed),
        state.metrics.webhook_failed.load(Ordering::Relaxed),
        state.metrics.webhook_rate_limited.load(Ordering::Relaxed)
    )
}

async fn enforce_rate_limit(
    State(state): State<PaymentHttpState>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    if request.uri().path() != "/webhooks/paypal" {
        return Ok(next.run(request).await);
    }
    let mut conn = state
        .redis
        .get()
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    let minute = chrono::Utc::now().format("%Y%m%d%H%M").to_string();
    let key = format!("payment:webhook:ratelimit:{minute}");
    let lua = r#"
        local count = redis.call('INCR', KEYS[1])
        if count == 1 then
            redis.call('EXPIRE', KEYS[1], ARGV[1])
        end
        return count
    "#;
    let count: usize = deadpool_redis::redis::cmd("EVAL")
        .arg(lua)
        .arg(1usize)
        .arg(&key)
        .arg(90u64)
        .query_async(&mut *conn)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    if count > state.max_requests_per_minute {
        state
            .metrics
            .webhook_rate_limited
            .fetch_add(1, Ordering::Relaxed);
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }
    Ok(next.run(request).await)
}

async fn paypal_webhook(
    State(state): State<PaymentHttpState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, (StatusCode, String)> {
    state
        .metrics
        .webhook_received
        .fetch_add(1, Ordering::Relaxed);
    let event = serde_json::from_slice::<Value>(&body).map_err(|e| {
        state.metrics.webhook_failed.fetch_add(1, Ordering::Relaxed);
        (StatusCode::BAD_REQUEST, e.to_string())
    })?;
    let webhook_headers = PayPalWebhookHeaders {
        transmission_id: header(&headers, "PAYPAL-TRANSMISSION-ID", &state.metrics)?,
        transmission_time: header(&headers, "PAYPAL-TRANSMISSION-TIME", &state.metrics)?,
        cert_url: header(&headers, "PAYPAL-CERT-URL", &state.metrics)?,
        auth_algo: header(&headers, "PAYPAL-AUTH-ALGO", &state.metrics)?,
        transmission_sig: header(&headers, "PAYPAL-TRANSMISSION-SIG", &state.metrics)?,
    };
    state
        .paypal_webhook
        .execute(webhook_headers, event)
        .await
        .map_err(|e| {
            state.metrics.webhook_failed.fetch_add(1, Ordering::Relaxed);
            tracing::warn!(error = %e, "paypal webhook rejected");
            (StatusCode::UNAUTHORIZED, e.to_string())
        })?;
    state
        .metrics
        .webhook_success
        .fetch_add(1, Ordering::Relaxed);
    Ok(StatusCode::OK)
}

fn header(
    headers: &HeaderMap,
    name: &'static str,
    metrics: &PaymentHttpMetrics,
) -> Result<String, (StatusCode, String)> {
    headers
        .get(name)
        .ok_or_else(|| {
            metrics.webhook_failed.fetch_add(1, Ordering::Relaxed);
            (
                StatusCode::BAD_REQUEST,
                format!("{name} header is required"),
            )
        })?
        .to_str()
        .map(|value| value.to_string())
        .map_err(|_| {
            metrics.webhook_failed.fetch_add(1, Ordering::Relaxed);
            (StatusCode::BAD_REQUEST, format!("{name} header is invalid"))
        })
}
