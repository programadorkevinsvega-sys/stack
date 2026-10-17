use crate::application::service::{ApiGatewayApplication, ChatCompletionRequest, EmbeddingRequest};
use crate::domain::errors::GatewayError;
use axum::extract::{ConnectInfo, DefaultBodyLimit, State};
use axum::http::{HeaderMap, HeaderName, StatusCode};
use axum::middleware::from_fn;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json};
use common::RequestId;
use futures_util::StreamExt;
use metrics::{describe_histogram, histogram, Unit};
use metrics_exporter_prometheus::PrometheusHandle;
use serde_json::json;
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::{Duration, Instant};

const X_REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

async fn security_headers(
    req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    let mut response = next.run(req).await;
    let headers = response.headers_mut();
    headers.insert("x-content-type-options", "nosniff".try_into().unwrap());
    headers.insert("x-frame-options", "DENY".try_into().unwrap());
    headers.insert(
        "strict-transport-security",
        "max-age=31536000; includeSubDomains".try_into().unwrap(),
    );
    response
}

async fn request_id_middleware(
    mut req: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    let request_id = RequestId::new();
    req.extensions_mut().insert(request_id);
    let mut response = next.run(req).await;
    if let Ok(value) = request_id.to_string().try_into() {
        response.headers_mut().insert(X_REQUEST_ID.clone(), value);
    }
    response
}

#[derive(Clone)]
pub struct HttpState {
    pub app: Arc<ApiGatewayApplication>,
    pub metrics: Arc<PrometheusHandle>,
}

pub fn router(state: HttpState) -> axum::Router {
    describe_histogram!(
        "gateway_request_duration_seconds",
        Unit::Seconds,
        "Request latency in seconds for gateway hot-path endpoints"
    );

    axum::Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics_handler))
        .route("/v1/chat/completions", post(chat_completions))
        .route("/v1/embeddings", post(embeddings))
        .layer(from_fn(security_headers))
        .layer(from_fn(request_id_middleware))
        .layer(DefaultBodyLimit::max(10 * 1024 * 1024))
        .layer(from_fn(common::observability::http_trace_middleware))
        .with_state(state)
}

async fn health() -> &'static str {
    "ok"
}

async fn metrics_handler(State(state): State<HttpState>) -> String {
    state.metrics.render()
}

fn client_ip(connect_info: SocketAddr, headers: &HeaderMap) -> String {
    if let Some(xff) = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok()) {
        if let Some(ip) = xff
            .split(',')
            .next()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
        {
            return ip.to_string();
        }
    }
    connect_info.ip().to_string()
}

async fn chat_completions(
    State(state): State<HttpState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(request_id): Extension<RequestId>,
    headers: HeaderMap,
    Json(request): Json<ChatCompletionRequest>,
) -> Response {
    let start = Instant::now();
    let is_stream = request.stream;
    let source_ip = client_ip(addr, &headers);
    let response = handle_chat(&state, &headers, &source_ip, request, request_id).await;
    let label = if is_stream {
        "chat_completion_stream"
    } else {
        "chat_completion"
    };
    histogram!(
        "gateway_request_duration_seconds",
        "endpoint" => label,
        "status" => "ok"
    )
    .record(start.elapsed().as_secs_f64());
    response
}

async fn embeddings(
    State(state): State<HttpState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    Extension(request_id): Extension<RequestId>,
    headers: HeaderMap,
    Json(request): Json<EmbeddingRequest>,
) -> Response {
    let start = Instant::now();
    let source_ip = client_ip(addr, &headers);
    let response = handle_embeddings(&state, &headers, &source_ip, request, request_id).await;
    histogram!(
        "gateway_request_duration_seconds",
        "endpoint" => "embeddings",
        "status" => "ok"
    )
    .record(start.elapsed().as_secs_f64());
    response
}

async fn handle_chat(
    state: &HttpState,
    headers: &HeaderMap,
    source_ip: &str,
    request: ChatCompletionRequest,
    request_id: RequestId,
) -> Response {
    let api_key = match bearer(headers) {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    if request.stream {
        match state
            .app
            .proxy_chat_completion_stream(
                &api_key,
                Some(source_ip.to_string()),
                request,
                request_id,
            )
            .await
        {
            Ok(stream) => {
                let events = stream.map(|item| {
                    let data = match item {
                        Ok(data) => data,
                        Err(error) => json!(error.envelope()).to_string(),
                    };
                    Ok::<Event, Infallible>(Event::default().data(data))
                });
                Sse::new(events)
                    .keep_alive(
                        KeepAlive::new()
                            .interval(Duration::from_secs(15))
                            .text(":keepalive"),
                    )
                    .into_response()
            }
            Err(error) => error_response(error),
        }
    } else {
        match state
            .app
            .proxy_chat_completion(&api_key, Some(source_ip.to_string()), request, request_id)
            .await
        {
            Ok(response) => (StatusCode::OK, Json(response)).into_response(),
            Err(error) => error_response(error),
        }
    }
}

async fn handle_embeddings(
    state: &HttpState,
    headers: &HeaderMap,
    source_ip: &str,
    request: EmbeddingRequest,
    request_id: RequestId,
) -> Response {
    let api_key = match bearer(headers) {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    match state
        .app
        .proxy_embeddings(&api_key, Some(source_ip.to_string()), request, request_id)
        .await
    {
        Ok(response) => (StatusCode::OK, Json(response)).into_response(),
        Err(error) => error_response(error),
    }
}

fn bearer(headers: &HeaderMap) -> Result<String, GatewayError> {
    let value = headers
        .get("authorization")
        .ok_or(GatewayError::AuthenticationRequired)?
        .to_str()
        .map_err(|_| GatewayError::AuthenticationRequired)?;
    value
        .strip_prefix("Bearer ")
        .or_else(|| value.strip_prefix("bearer "))
        .map(ToOwned::to_owned)
        .ok_or(GatewayError::AuthenticationRequired)
}

fn error_response(error: GatewayError) -> Response {
    let status = error.status_code();
    (status, Json(json!(error.envelope()))).into_response()
}
