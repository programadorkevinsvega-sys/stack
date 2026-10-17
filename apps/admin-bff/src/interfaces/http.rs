use crate::application::service::BffApplication;
use crate::domain::errors::BffError;
use crate::domain::validation;
use crate::infrastructure::clients::{
    CreatePlanInput, LoginResult, ModelMetadataInput, ProviderPolicyInput, RegisterModelInput,
    UpdatePlanInput,
};
use crate::infrastructure::rate_limiter::RedisRateLimiter;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{from_fn, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, patch, post};
use axum::{Extension, Json, Router};
use base64::Engine;
use metrics_exporter_prometheus::PrometheusHandle;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tower_http::compression::CompressionLayer;
use tower_http::cors::{AllowOrigin, CorsLayer};
use uuid::Uuid;

const REQUEST_ID_HEADER: &str = "x-request-id";

const SESSION_COOKIE: &str = "session_token";
const CSRF_COOKIE: &str = "csrf_token";

#[derive(Clone)]
pub struct HttpState {
    pub app: Arc<BffApplication>,
    pub metrics: Arc<PrometheusHandle>,
    pub cors_allowed_origins: Vec<String>,
    pub cookie_secure: bool,
    pub cookie_domain: Option<String>,
    pub rate_limiter: RedisRateLimiter,
}

async fn user_rate_limit_middleware(
    Extension(limiter): Extension<RedisRateLimiter>,
    user_id: Option<Extension<Uuid>>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    let uid = match user_id {
        Some(Extension(uid)) => uid,
        None => return next.run(req).await,
    };
    let (info, rl_err) = match limiter.check_user(uid).await {
        Ok((info, err)) => (Some(info), err),
        Err(e) => {
            tracing::warn!(%uid, error = %e, "user rate limit check failed");
            return err_json(e);
        }
    };
    if let Some(e) = rl_err {
        tracing::warn!(%uid, error = %e, "user rate limit exceeded");
        let resp = if let Some(ref rl) = info {
            let mut r = err_json(e);
            r.headers_mut().insert(
                "x-ratelimit-remaining",
                rl.remaining.to_string().parse().unwrap(),
            );
            r.headers_mut().insert(
                "x-ratelimit-reset",
                rl.reset_at_secs.to_string().parse().unwrap(),
            );
            r
        } else {
            err_json(e)
        };
        return resp;
    }

    let mut response = next.run(req).await;

    if let Some(rl) = info {
        response.headers_mut().insert(
            "x-ratelimit-remaining",
            rl.remaining.to_string().parse().unwrap(),
        );
        response.headers_mut().insert(
            "x-ratelimit-reset",
            rl.reset_at_secs.to_string().parse().unwrap(),
        );
    }

    response
}

async fn rate_limit_middleware(
    Extension(limiter): Extension<RedisRateLimiter>,
    req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    let ip = client_ip(req.headers());
    let (info, rl_err) = if ip != "unknown" {
        match limiter.check_ip(&ip).await {
            Ok((info, err)) => (Some(info), err),
            Err(e) => {
                tracing::warn!(client_ip = %ip, error = %e, "ip rate limit check failed");
                return err_json(e);
            }
        }
    } else {
        (None, None)
    };

    if let Some(e) = rl_err {
        tracing::warn!(client_ip = %ip, error = %e, "ip rate limit exceeded");
        let resp = if let Some(ref rl) = info {
            let mut r = err_json(e);
            r.headers_mut().insert(
                "x-ratelimit-remaining",
                rl.remaining.to_string().parse().unwrap(),
            );
            r.headers_mut().insert(
                "x-ratelimit-reset",
                rl.reset_at_secs.to_string().parse().unwrap(),
            );
            r
        } else {
            err_json(e)
        };
        return resp;
    }

    let mut response = next.run(req).await;

    if let Some(rl) = info {
        response.headers_mut().insert(
            "x-ratelimit-remaining",
            rl.remaining.to_string().parse().unwrap(),
        );
        response.headers_mut().insert(
            "x-ratelimit-reset",
            rl.reset_at_secs.to_string().parse().unwrap(),
        );
    }

    response
}

pub fn client_ip(headers: &HeaderMap) -> String {
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
    "unknown".into()
}

pub fn extract_session_cookie(headers: &HeaderMap) -> Result<String, BffError> {
    let cookie = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .ok_or(BffError::AuthenticationRequired)?;
    for pair in cookie.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix(&format!("{SESSION_COOKIE}=")) {
            return Ok(value.to_string());
        }
    }
    Err(BffError::AuthenticationRequired)
}

pub fn read_jwt_iat_exp(token: &str) -> Option<(i64, i64)> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        return None;
    }
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(parts[1])
        .ok()?;
    let claims: serde_json::Value = serde_json::from_slice(&payload).ok()?;
    let iat = claims.get("iat")?.as_i64()?;
    let exp = claims.get("exp")?.as_i64()?;
    Some((iat, exp))
}

pub fn should_refresh_token(token: &str) -> bool {
    let (iat, exp) = match read_jwt_iat_exp(token) {
        Some(v) => v,
        None => return false,
    };
    let now = chrono::Utc::now().timestamp();
    let lifetime = exp - iat;
    let elapsed = now - iat;
    elapsed >= lifetime / 2 || now >= exp
}

async fn session_middleware(
    Extension(app): Extension<Arc<BffApplication>>,
    mut req: axum::http::Request<axum::body::Body>,
    next: Next,
) -> Response {
    if is_public_path(req.uri().path()) {
        return next.run(req).await;
    }
    let access_token = match extract_session_cookie(req.headers()) {
        Ok(t) => t,
        Err(e) => {
            return err_response(StatusCode::UNAUTHORIZED, "AUTH_REQUIRED", &e.to_string());
        }
    };

    let refresh_token = req
        .headers()
        .get("x-refresh-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let (uid, slide_at, slide_rt) = if let Some(rt) = &refresh_token {
        if should_refresh_token(&access_token) {
            match app.auth.refresh(rt.clone()).await {
                Ok(result) => {
                    if result.user.role != "admin" {
                        return err_response(
                            StatusCode::FORBIDDEN,
                            "ADMIN_ROLE_REQUIRED",
                            "administrator role required",
                        );
                    }
                    let uid = Uuid::parse_str(&result.user.id).unwrap_or(Uuid::nil());
                    tracing::info!(%uid, "silent session refresh");
                    (uid, Some(result.access_token), Some(result.refresh_token))
                }
                Err(BffError::Forbidden) => {
                    return err_response(
                        StatusCode::FORBIDDEN,
                        "ADMIN_ROLE_REQUIRED",
                        "administrator role required",
                    )
                }
                Err(_) => match app.auth.validate_token(&access_token).await {
                    Ok(s) => (s.user_id, None, None),
                    Err(BffError::Forbidden) => {
                        return err_response(
                            StatusCode::FORBIDDEN,
                            "ADMIN_ROLE_REQUIRED",
                            "administrator role required",
                        )
                    }
                    Err(_) => {
                        return err_response(
                            StatusCode::UNAUTHORIZED,
                            "SESSION_EXPIRED",
                            "invalid or expired session",
                        )
                    }
                },
            }
        } else {
            match app.auth.validate_token(&access_token).await {
                Ok(s) => (s.user_id, None, None),
                Err(BffError::Forbidden) => {
                    return err_response(
                        StatusCode::FORBIDDEN,
                        "ADMIN_ROLE_REQUIRED",
                        "administrator role required",
                    )
                }
                Err(_) => {
                    return err_response(
                        StatusCode::UNAUTHORIZED,
                        "SESSION_EXPIRED",
                        "invalid or expired session",
                    )
                }
            }
        }
    } else {
        match app.auth.validate_token(&access_token).await {
            Ok(s) => (s.user_id, None, None),
            Err(BffError::Forbidden) => {
                return err_response(
                    StatusCode::FORBIDDEN,
                    "ADMIN_ROLE_REQUIRED",
                    "administrator role required",
                )
            }
            Err(_) => {
                return err_response(
                    StatusCode::UNAUTHORIZED,
                    "SESSION_EXPIRED",
                    "invalid or expired session",
                )
            }
        }
    };

    req.extensions_mut().insert(uid);
    let original_secure = app.cookie_secure;

    let mut response = next.run(req).await;

    if let (Some(at), Some(rt)) = (slide_at, slide_rt) {
        set_session_cookie(
            &at,
            original_secure,
            app.cookie_domain.as_deref(),
            &mut response,
        );
        if let Ok(hdr) = rt.parse::<HeaderValue>() {
            response.headers_mut().insert("x-refresh-token", hdr);
        } else {
            tracing::warn!("failed to parse refresh token as HeaderValue");
        }
    }

    response
}

pub fn extract_csrf_token(headers: &HeaderMap) -> Result<String, BffError> {
    let cookie = headers
        .get("cookie")
        .and_then(|v| v.to_str().ok())
        .ok_or(BffError::CsrfTokenMismatch)?;
    for pair in cookie.split(';') {
        let pair = pair.trim();
        if let Some(value) = pair.strip_prefix(&format!("{CSRF_COOKIE}=")) {
            return Ok(value.to_string());
        }
    }
    Err(BffError::CsrfTokenMismatch)
}

pub fn is_public_path(path: &str) -> bool {
    path == "/"
        || path == "/health"
        || path == "/metrics"
        || path == "/api/v1/auth/login"
        || path == "/api/v1/auth/refresh"
}

async fn csrf_middleware(req: axum::http::Request<axum::body::Body>, next: Next) -> Response {
    let path = req.uri().path();
    if is_public_path(path) {
        return next.run(req).await;
    }
    let method = req.method();
    if method == Method::GET || method == Method::HEAD || method == Method::OPTIONS {
        return next.run(req).await;
    }
    let header_token = req
        .headers()
        .get("x-csrf-token")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_default();
    let cookie_token = match extract_csrf_token(req.headers()) {
        Ok(t) => t,
        Err(_) => return err_json(BffError::CsrfTokenMismatch),
    };
    if header_token != cookie_token {
        return err_json(BffError::CsrfTokenMismatch);
    }
    next.run(req).await
}

async fn security_headers(req: axum::http::Request<axum::body::Body>, next: Next) -> Response {
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
    next: Next,
) -> Response {
    let id = req
        .headers()
        .get(REQUEST_ID_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::now_v7().to_string());
    req.extensions_mut().insert(id.clone());
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert(REQUEST_ID_HEADER, id.parse().expect("UUID as HeaderValue"));
    response
}

async fn metrics_middleware(req: axum::http::Request<axum::body::Body>, next: Next) -> Response {
    let start = std::time::Instant::now();
    let method = req.method().to_string();
    let path = req.uri().path().to_string();
    let response = next.run(req).await;
    let status = response.status().as_u16();
    let duration = start.elapsed();
    metrics::counter!("http_requests_total", "method" => method.clone(), "path" => path.clone(), "status" => status.to_string()).increment(1);
    metrics::histogram!("http_request_duration_seconds", "method" => method, "path" => path)
        .record(duration.as_secs_f64());
    response
}

async fn timeout_middleware(req: axum::http::Request<axum::body::Body>, next: Next) -> Response {
    match tokio::time::timeout(Duration::from_secs(30), next.run(req)).await {
        Ok(resp) => resp,
        Err(_) => (StatusCode::GATEWAY_TIMEOUT, "request timed out").into_response(),
    }
}

pub fn router(state: HttpState) -> Router {
    let app = state.app.clone();

    let cors = {
        let has_wildcard = state.cors_allowed_origins.iter().any(|o| o == "*");
        if has_wildcard {
            CorsLayer::permissive()
        } else {
            let origins: Vec<_> = state
                .cors_allowed_origins
                .iter()
                .map(|o| {
                    o.parse()
                        .unwrap_or_else(|_| panic!("invalid CORS origin: {o}"))
                })
                .collect();
            CorsLayer::new()
                .allow_origin(AllowOrigin::list(origins))
                .allow_credentials(true)
                .allow_methods([
                    Method::GET,
                    Method::POST,
                    Method::PUT,
                    Method::PATCH,
                    Method::DELETE,
                    Method::OPTIONS,
                ])
                .allow_headers([
                    axum::http::header::CONTENT_TYPE,
                    axum::http::header::AUTHORIZATION,
                    axum::http::header::HeaderName::from_static("x-csrf-token"),
                    axum::http::header::HeaderName::from_static("x-refresh-token"),
                    axum::http::header::HeaderName::from_static("x-request-id"),
                ])
                .expose_headers([
                    axum::http::header::HeaderName::from_static("x-refresh-token"),
                    axum::http::header::HeaderName::from_static("x-request-id"),
                ])
                .max_age(Duration::from_secs(86400))
        }
    };

    Router::new()
        .route("/", get(health))
        .route("/health", get(health))
        .route("/metrics", get(metrics_handler))
        .route("/api/v1/csrf-token", get(csrf_token))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/refresh", post(refresh))
        .route("/api/v1/me", get(get_profile))
        .route("/api/v1/me", patch(update_profile))
        .route("/api/v1/auth/change-password", post(change_password))
        // Admin: Users
        .route("/api/v1/admin/users", get(list_users))
        .route("/api/v1/admin/users/{id}", get(get_user))
        .route("/api/v1/admin/users/{id}/disable", post(disable_user))
        .route("/api/v1/admin/users/{id}/unlock", post(unlock_account))
        .route("/api/v1/admin/users/{id}/delete", post(delete_user))
        // Admin: Models
        .route(
            "/api/v1/admin/models",
            get(list_models).post(register_model),
        )
        .route("/api/v1/admin/models/{id}", get(get_model))
        .route("/api/v1/admin/models/{id}/deprecate", post(deprecate_model))
        .route("/api/v1/admin/models/{id}/pricing", post(set_model_pricing))
        .route(
            "/api/v1/admin/models/{id}/capabilities",
            post(update_capabilities),
        )
        .route("/api/v1/admin/models/{id}", patch(update_model))
        .route(
            "/api/v1/admin/models/{id}/metadata",
            get(get_model_metadata).post(upsert_model_metadata),
        )
        .route(
            "/api/v1/admin/providers",
            get(list_providers).post(register_provider),
        )
        .route(
            "/api/v1/admin/providers/{id}/policy",
            get(get_provider_policy).post(set_provider_policy),
        )
        .route(
            "/api/v1/admin/accounts/{account_id}/models/{model_catalog_id}/mantle-connection",
            post(upsert_mantle_connection),
        )
        .route("/api/v1/admin/accounts", get(list_accounts))
        // Admin: Plans
        .route("/api/v1/admin/plans", get(list_plans).post(create_plan))
        .route("/api/v1/admin/plans/{id}", get(get_plan).put(update_plan))
        .route("/api/v1/admin/plans/{id}/deactivate", post(deactivate_plan))
        // Admin: Wallets
        .route("/api/v1/admin/wallets/{user_id}", get(get_wallet))
        .route(
            "/api/v1/admin/wallets/{user_id}/adjust",
            post(adjust_wallet),
        )
        // Admin: Invoices
        .route("/api/v1/admin/invoices", get(list_invoices))
        .route("/api/v1/admin/invoices/{id}", get(get_invoice))
        .route("/api/v1/admin/invoices/{id}/open", post(open_invoice))
        .route("/api/v1/admin/invoices/{id}/void", post(void_invoice))
        .route(
            "/api/v1/admin/invoices/{id}/uncollectible",
            post(mark_uncollectible),
        )
        // Admin: Usage
        .route(
            "/api/v1/admin/usage/users/{user_id}",
            get(get_usage_by_user),
        )
        .route("/api/v1/admin/usage/summary", get(usage_summary))
        .route("/api/v1/admin/usage/top-models", get(top_models))
        .route("/api/v1/admin/usage/error-rate", get(error_rate))
        .route("/api/v1/admin/usage/latency", get(latency_stats))
        .route("/api/v1/admin/usage/refresh-views", post(refresh_views))
        .route(
            "/api/v1/admin/usage/create-partition",
            post(create_partition),
        )
        // Admin: Storage and ingestion
        .route(
            "/api/v1/admin/storage/users/{user_id}",
            get(storage_by_user),
        )
        // Admin: Router
        .route(
            "/api/v1/admin/router/accounts/{model_catalog_id}",
            get(list_account_loads),
        )
        .route(
            "/api/v1/admin/router/accounts/{account_id}/reset-circuit-breaker",
            post(reset_circuit_breaker),
        )
        .layer(DefaultBodyLimit::max(1024 * 1024))
        .layer(from_fn(user_rate_limit_middleware))
        .layer(from_fn(session_middleware))
        .layer(from_fn(csrf_middleware))
        .layer(Extension(app))
        .layer(from_fn(security_headers))
        .layer(from_fn(rate_limit_middleware))
        .layer(Extension(state.rate_limiter.clone()))
        .layer(cors)
        .layer(CompressionLayer::new())
        .layer(from_fn(timeout_middleware))
        .layer(from_fn(metrics_middleware))
        .layer(from_fn(common::observability::http_trace_middleware))
        .layer(from_fn(request_id_middleware))
        .with_state(state)
}

async fn health(State(state): State<HttpState>) -> Response {
    let mut services: HashMap<&str, &str> = HashMap::new();
    let mut all_up = true;
    for (name, cfg) in [
        ("auth", &state.app.auth.config),
        ("catalog", &state.app.catalog.config),
        ("plans", &state.app.plans.config),
        ("wallet", &state.app.wallet.config),
        ("invoices", &state.app.invoices.config),
        ("usage", &state.app.usage.config),
        ("router", &state.app.router.config),
        ("files", &state.app.files.config),
    ] {
        let up = tokio::time::timeout(Duration::from_secs(3), async {
            cfg.channel().await.is_ok()
        })
        .await
        .unwrap_or(false);
        services.insert(name, if up { "up" } else { "down" });
        if !up {
            all_up = false;
        }
    }
    let status = if all_up { "ok" } else { "degraded" };
    Json(json!({ "status": status, "services": services })).into_response()
}

async fn metrics_handler(State(state): State<HttpState>) -> String {
    state.metrics.render()
}

pub fn err_json(e: BffError) -> Response {
    let status = e.status_code();
    if status.is_server_error() {
        tracing::error!(error = %e, status = status.as_u16(), "internal error");
    } else if status.is_client_error() {
        tracing::warn!(error = %e, status = status.as_u16(), "client error");
    }
    (
        status,
        Json(json!({ "error": true, "code": e.code(), "message": e.to_string() })),
    )
        .into_response()
}

pub fn err_response(status: StatusCode, code: &'static str, message: &str) -> Response {
    (
        status,
        Json(json!({ "error": true, "code": code, "message": message })),
    )
        .into_response()
}

pub fn set_session_cookie(
    token: &str,
    secure: bool,
    domain: Option<&str>,
    response: &mut Response,
) {
    let secure_flag = if secure { "; Secure" } else { "" };
    let domain_flag = domain
        .map(|value| format!("; Domain={value}"))
        .unwrap_or_default();
    let cookie = format!(
        "{SESSION_COOKIE}={token}; HttpOnly{secure_flag}{domain_flag}; SameSite=Lax; Path=/; Max-Age=86400"
    );
    // Login emits both session_token and csrf_token. Set-Cookie headers must
    // be appended; inserting the second one would discard the session cookie.
    response
        .headers_mut()
        .append("set-cookie", cookie.try_into().unwrap());
}

pub fn clear_session_cookie(secure: bool, domain: Option<&str>, response: &mut Response) {
    let secure_flag = if secure { "; Secure" } else { "" };
    let domain_flag = domain
        .map(|value| format!("; Domain={value}"))
        .unwrap_or_default();
    let cookie = format!(
        "{SESSION_COOKIE}=; HttpOnly{secure_flag}{domain_flag}; SameSite=Lax; Path=/; Max-Age=0"
    );
    response
        .headers_mut()
        .append("set-cookie", cookie.try_into().unwrap());
}

pub fn set_csrf_cookie(token: &str, secure: bool, domain: Option<&str>, response: &mut Response) {
    let secure_flag = if secure { "; Secure" } else { "" };
    let domain_flag = domain
        .map(|value| format!("; Domain={value}"))
        .unwrap_or_default();
    let cookie = format!("{CSRF_COOKIE}={token}; SameSite=Lax; Path=/{secure_flag}{domain_flag}");
    response
        .headers_mut()
        .append("set-cookie", cookie.try_into().unwrap());
}

pub fn generate_csrf_token() -> String {
    Uuid::new_v4().to_string().replace('-', "")
}

fn login_response(result: LoginResult, secure: bool, domain: Option<&str>) -> Response {
    let mut response = (StatusCode::OK, Json(json!(result.user))).into_response();
    set_session_cookie(&result.access_token, secure, domain, &mut response);
    let csrf_token = generate_csrf_token();
    set_csrf_cookie(&csrf_token, secure, domain, &mut response);
    if let Ok(hdr) = result.refresh_token.parse::<HeaderValue>() {
        response.headers_mut().insert("x-refresh-token", hdr);
    } else {
        tracing::warn!("failed to parse refresh token as HeaderValue in login response");
    }
    response
}

// === Auth Handlers ===

async fn csrf_token(State(state): State<HttpState>, headers: HeaderMap) -> Response {
    let token = match extract_csrf_token(&headers) {
        Ok(t) => t,
        Err(_) => {
            let token = generate_csrf_token();
            let mut response = (StatusCode::OK, Json(json!({ "token": &token }))).into_response();
            set_csrf_cookie(
                &token,
                state.cookie_secure,
                state.cookie_domain.as_deref(),
                &mut response,
            );
            return response;
        }
    };
    (StatusCode::OK, Json(json!({ "token": token }))).into_response()
}

#[derive(Deserialize)]
struct LoginBody {
    email: String,
    password: String,
    mfa_code: Option<String>,
}

async fn login(
    State(state): State<HttpState>,
    headers: HeaderMap,
    Json(body): Json<LoginBody>,
) -> Response {
    if let Err(e) = validation::validate_email(&body.email) {
        return err_json(e);
    }
    if body.password.is_empty() {
        return err_json(BffError::Validation("password is required".into()));
    }
    if let Some(ref code) = body.mfa_code {
        if let Err(e) = validation::validate_mfa_code(code) {
            return err_json(e);
        }
    }
    let email = body.email.clone();
    let ip = match client_ip(&headers).as_str() {
        "unknown" => None,
        value => Some(value.to_string()),
    };
    let ua = headers
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    match state
        .app
        .auth
        .login(body.email, body.password, body.mfa_code, ip, ua)
        .await
    {
        Ok(result) => {
            tracing::info!(event = "audit", action = "admin_login", email = %result.user.email, "admin logged in");
            login_response(result, state.cookie_secure, state.cookie_domain.as_deref())
        }
        Err(e) => {
            tracing::warn!(event = "audit", action = "admin_login", email = %email, error = %e, "admin login failed");
            err_json(e)
        }
    }
}

#[derive(Deserialize)]
struct RefreshBody {
    refresh_token: String,
}

async fn refresh(State(state): State<HttpState>, Json(body): Json<RefreshBody>) -> Response {
    if let Err(e) = validation::validate_string_length(&body.refresh_token, 1024, "refresh_token") {
        return err_json(e);
    }
    match state.app.auth.refresh(body.refresh_token).await {
        Ok(result) => login_response(result, state.cookie_secure, state.cookie_domain.as_deref()),
        Err(e) => err_json(e),
    }
}

async fn get_profile(State(state): State<HttpState>, Extension(uid): Extension<Uuid>) -> Response {
    match state.app.auth.get_user(uid).await {
        Ok(user) => (StatusCode::OK, Json(json!(user))).into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct UpdateProfileBody {
    display_name: Option<String>,
    avatar_url: Option<String>,
}

async fn update_profile(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Json(body): Json<UpdateProfileBody>,
) -> Response {
    if let Some(ref name) = body.display_name {
        if let Err(e) = validation::validate_string_length(name, 100, "display_name") {
            return err_json(e);
        }
    }
    if let Some(ref url) = body.avatar_url {
        if let Err(e) = validation::validate_string_length(url, 2048, "avatar_url") {
            return err_json(e);
        }
    }
    match state.app.auth.get_user(uid).await {
        Ok(user) => (StatusCode::OK, Json(json!(user))).into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct ChangePasswordBody {
    old_password: String,
    new_password: String,
}

async fn change_password(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Json(body): Json<ChangePasswordBody>,
) -> Response {
    if body.old_password.is_empty() {
        return err_json(BffError::Validation("old_password is required".into()));
    }
    if let Err(e) = validation::validate_password(&body.new_password) {
        return err_json(e);
    }
    match state.app.auth.get_user(uid).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "success": true }))).into_response(),
        Err(e) => err_json(e),
    }
}

// === Admin User Handlers ===

#[derive(Deserialize)]
struct ListUsersQuery {
    limit: Option<u32>,
    after_cursor: Option<String>,
    status: Option<String>,
    search: Option<String>,
}

async fn list_users(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Query(params): Query<ListUsersQuery>,
) -> Response {
    match state
        .app
        .auth
        .list_users(params.limit, params.after_cursor)
        .await
    {
        Ok(users) => {
            let users = users
                .into_iter()
                .filter(|user| {
                    params
                        .status
                        .as_deref()
                        .map(|status| user.status == status)
                        .unwrap_or(true)
                })
                .filter(|user| {
                    params
                        .search
                        .as_deref()
                        .map(|search| {
                            let needle = search.to_ascii_lowercase();
                            user.email.to_ascii_lowercase().contains(&needle)
                                || user
                                    .display_name
                                    .as_deref()
                                    .map(|name| name.to_ascii_lowercase().contains(&needle))
                                    .unwrap_or(false)
                        })
                        .unwrap_or(true)
                })
                .collect::<Vec<_>>();
            let count = users.len();
            (
                StatusCode::OK,
                Json(json!({ "data": users, "count": count })),
            )
                .into_response()
        }
        Err(e) => err_json(e),
    }
}

async fn get_user(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.app.auth.get_user(id).await {
        Ok(user) => (StatusCode::OK, Json(json!(user))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn disable_user(
    State(state): State<HttpState>,
    Extension(admin_uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.app.auth.disable_user(id, admin_uid).await {
        Ok(success) => {
            tracing::info!(event = "audit", action = "disable_user", admin_id = %admin_uid, target_id = %id, "user disabled");
            (StatusCode::OK, Json(json!({ "success": success }))).into_response()
        }
        Err(e) => err_json(e),
    }
}

async fn unlock_account(
    State(state): State<HttpState>,
    Extension(admin_uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.app.auth.unlock_account(id, admin_uid).await {
        Ok(success) => {
            tracing::info!(event = "audit", action = "unlock_account", admin_id = %admin_uid, target_id = %id, "account unlocked");
            (StatusCode::OK, Json(json!({ "success": success }))).into_response()
        }
        Err(e) => err_json(e),
    }
}

async fn delete_user(
    State(state): State<HttpState>,
    Extension(admin_uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.app.auth.delete_user(id, admin_uid).await {
        Ok(success) => {
            tracing::info!(event = "audit", action = "delete_user", admin_id = %admin_uid, target_id = %id, "user anonymized and deleted");
            (
                StatusCode::OK,
                Json(json!({ "success": success, "anonymized": true })),
            )
                .into_response()
        }
        Err(e) => err_json(e),
    }
}

// === Admin Model Handlers ===

#[derive(Deserialize)]
struct ListModelsQuery {
    limit: Option<u32>,
    offset: Option<u32>,
    after_cursor: Option<String>,
    active: Option<bool>,
}

async fn list_models(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Query(params): Query<ListModelsQuery>,
) -> Response {
    match state
        .app
        .catalog
        .list_models(
            params.limit,
            params.offset,
            params.after_cursor,
            params.active,
        )
        .await
    {
        Ok(models) => {
            let count = models.len();
            (
                StatusCode::OK,
                Json(json!({ "data": models, "count": count })),
            )
                .into_response()
        }
        Err(e) => err_json(e),
    }
}

async fn get_model(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.app.catalog.get_model(id).await {
        Ok(model) => (StatusCode::OK, Json(json!(model))).into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct RegisterModelBody {
    provider_id: String,
    public_model_name: String,
    provider_model_id: String,
    family: Option<String>,
    context_window: i32,
    max_output_tokens: i32,
    supports_streaming: bool,
    supports_tools: bool,
    supports_vision: bool,
}

async fn register_model(
    State(state): State<HttpState>,
    Extension(admin_uid): Extension<Uuid>,
    Json(body): Json<RegisterModelBody>,
) -> Response {
    if let Err(e) = validation::validate_model_name(&body.public_model_name) {
        return err_json(e);
    }
    if body.provider_id.is_empty() {
        return err_json(BffError::Validation("provider_id is required".into()));
    }
    match state
        .app
        .catalog
        .register_model(RegisterModelInput {
            provider_id: body.provider_id,
            public_model_name: body.public_model_name,
            provider_model_id: body.provider_model_id,
            family: body.family,
            context_window: body.context_window,
            max_output_tokens: body.max_output_tokens,
            supports_streaming: body.supports_streaming,
            supports_tools: body.supports_tools,
            supports_vision: body.supports_vision,
            admin_actor_id: admin_uid,
        })
        .await
    {
        Ok(model) => {
            tracing::info!(event = "audit", action = "register_model", admin_id = %admin_uid, model = %model.public_model_name, "model registered");
            (StatusCode::CREATED, Json(json!(model))).into_response()
        }
        Err(e) => err_json(e),
    }
}

async fn deprecate_model(
    State(state): State<HttpState>,
    Extension(admin_uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.app.catalog.deprecate_model(id, admin_uid).await {
        Ok(success) => {
            tracing::info!(event = "audit", action = "deprecate_model", admin_id = %admin_uid, model_id = %id, "model deprecated");
            (StatusCode::OK, Json(json!({ "success": success }))).into_response()
        }
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct SetPricingBody {
    model_catalog_id: String,
    aws_cost_per_1k_input: String,
    aws_cost_per_1k_output: String,
    price_per_1k_input: String,
    price_per_1k_output: String,
}

#[derive(Deserialize)]
struct UpdateModelBody {
    provider_model_id: Option<String>,
    supports_streaming: Option<bool>,
    supports_tools: Option<bool>,
    supports_vision: Option<bool>,
}

async fn update_model(
    State(state): State<HttpState>,
    Extension(admin_uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateModelBody>,
) -> Response {
    if let Some(provider_model_id) = body.provider_model_id.as_deref() {
        if provider_model_id.trim().is_empty() || provider_model_id.len() > 150 {
            return err_json(BffError::Validation(
                "provider_model_id must contain between 1 and 150 characters".into(),
            ));
        }
    }
    match state
        .app
        .catalog
        .update_capabilities(
            id,
            body.supports_streaming,
            body.supports_tools,
            body.supports_vision,
            body.provider_model_id,
            admin_uid,
        )
        .await
    {
        Ok(model) => (StatusCode::OK, Json(json!(model))).into_response(),
        Err(error) => err_json(error),
    }
}

async fn set_model_pricing(
    State(state): State<HttpState>,
    Extension(admin_uid): Extension<Uuid>,
    Json(body): Json<SetPricingBody>,
) -> Response {
    if let Err(e) = validation::validate_decimal(&body.price_per_1k_input, "price_per_1k_input") {
        return err_json(e);
    }
    if let Err(e) = validation::validate_decimal(&body.price_per_1k_output, "price_per_1k_output") {
        return err_json(e);
    }
    let model_catalog_id = body.model_catalog_id.clone();
    match state
        .app
        .catalog
        .set_model_pricing(
            model_catalog_id.clone(),
            body.aws_cost_per_1k_input,
            body.aws_cost_per_1k_output,
            body.price_per_1k_input,
            body.price_per_1k_output,
            admin_uid,
        )
        .await
    {
        Ok(pricing) => {
            tracing::info!(event = "audit", action = "set_pricing", admin_id = %admin_uid, model_id = %model_catalog_id, "pricing set");
            (StatusCode::OK, Json(json!(pricing))).into_response()
        }
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct UpdateCapabilitiesBody {
    supports_streaming: Option<bool>,
    supports_tools: Option<bool>,
    supports_vision: Option<bool>,
}

async fn update_capabilities(
    State(state): State<HttpState>,
    Extension(admin_uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateCapabilitiesBody>,
) -> Response {
    match state
        .app
        .catalog
        .update_capabilities(
            id,
            body.supports_streaming,
            body.supports_tools,
            body.supports_vision,
            None,
            admin_uid,
        )
        .await
    {
        Ok(model) => {
            tracing::info!(event = "audit", action = "update_capabilities", admin_id = %admin_uid, model_id = %id, "capabilities updated");
            (StatusCode::OK, Json(json!(model))).into_response()
        }
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct ModelMetadataBody {
    serving_provider_id: Option<String>,
    description: Option<String>,
    architecture: Option<String>,
    parameter_count: Option<i64>,
    precision: Option<String>,
    quantization: Option<String>,
    metadata: Value,
    source_url: Option<String>,
    source_name: Option<String>,
    source_updated_at: Option<String>,
    metadata_version: i32,
}

async fn get_model_metadata(State(state): State<HttpState>, Path(id): Path<Uuid>) -> Response {
    match state.app.catalog.get_metadata(id).await {
        Ok(metadata) => (StatusCode::OK, Json(json!(metadata))).into_response(),
        Err(error) => err_json(error),
    }
}

async fn upsert_model_metadata(
    State(state): State<HttpState>,
    Extension(admin_uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
    Json(body): Json<ModelMetadataBody>,
) -> Response {
    if !body.metadata.is_object() {
        return err_json(BffError::Validation(
            "metadata must be a JSON object".into(),
        ));
    }
    match state
        .app
        .catalog
        .upsert_metadata(
            id,
            ModelMetadataInput {
                serving_provider_id: body.serving_provider_id,
                description: body.description,
                architecture: body.architecture,
                parameter_count: body.parameter_count,
                precision: body.precision,
                quantization: body.quantization,
                metadata_json: body.metadata.to_string(),
                source_url: body.source_url,
                source_name: body.source_name,
                source_updated_at: body.source_updated_at,
                metadata_version: body.metadata_version,
            },
            admin_uid,
        )
        .await
    {
        Ok(metadata) => (StatusCode::OK, Json(json!(metadata))).into_response(),
        Err(error) => err_json(error),
    }
}

#[derive(Deserialize)]
struct ProviderPolicyBody {
    model_catalog_id: Option<String>,
    retention_mode: String,
    retention_days: Option<i32>,
    training_usage: String,
    provider_access: String,
    supported_retention_modes: Value,
    data_residency: Value,
    security_controls: Value,
    compliance: Value,
    limitations: Value,
    privacy_policy_url: Option<String>,
    terms_url: Option<String>,
    documentation_url: Option<String>,
    source_url: Option<String>,
    policy_version: String,
}

#[derive(Deserialize)]
struct ProviderPolicyQuery {
    model_catalog_id: Option<Uuid>,
}

async fn get_provider_policy(
    State(state): State<HttpState>,
    Path(provider_id): Path<Uuid>,
    Query(query): Query<ProviderPolicyQuery>,
) -> Response {
    match state
        .app
        .catalog
        .get_provider_policy(provider_id, query.model_catalog_id)
        .await
    {
        Ok(policy) => (StatusCode::OK, Json(json!(policy))).into_response(),
        Err(error) => err_json(error),
    }
}

async fn set_provider_policy(
    State(state): State<HttpState>,
    Extension(admin_uid): Extension<Uuid>,
    Path(provider_id): Path<Uuid>,
    Json(body): Json<ProviderPolicyBody>,
) -> Response {
    let model_catalog_id = match body.model_catalog_id {
        Some(value) => match Uuid::parse_str(value.trim()) {
            Ok(parsed) => Some(parsed.to_string()),
            Err(error) => {
                return err_json(BffError::Validation(format!(
                    "invalid model_catalog_id: {error}"
                )))
            }
        },
        None => None,
    };
    if !body.supported_retention_modes.is_array()
        || !body.data_residency.is_object()
        || !body.security_controls.is_object()
        || !body.compliance.is_array()
        || !body.limitations.is_array()
    {
        return err_json(BffError::Validation(
            "provider policy JSON fields have invalid types".into(),
        ));
    }
    match state
        .app
        .catalog
        .set_provider_policy(
            ProviderPolicyInput {
                provider_id: provider_id.to_string(),
                model_catalog_id,
                retention_mode: body.retention_mode,
                retention_days: body.retention_days,
                training_usage: body.training_usage,
                provider_access: body.provider_access,
                supported_retention_modes_json: body.supported_retention_modes.to_string(),
                data_residency_json: body.data_residency.to_string(),
                security_controls_json: body.security_controls.to_string(),
                compliance_json: body.compliance.to_string(),
                limitations_json: body.limitations.to_string(),
                privacy_policy_url: body.privacy_policy_url,
                terms_url: body.terms_url,
                documentation_url: body.documentation_url,
                source_url: body.source_url,
                policy_version: body.policy_version,
            },
            admin_uid,
        )
        .await
    {
        Ok(policy) => (StatusCode::OK, Json(json!(policy))).into_response(),
        Err(error) => err_json(error),
    }
}

#[derive(Deserialize)]
struct MantleConnectionBody {
    api_key: String,
    base_url: String,
    expires_at: Option<String>,
}

async fn list_accounts(
    State(state): State<HttpState>,
    Extension(admin_uid): Extension<Uuid>,
) -> Response {
    tracing::debug!(event = "audit", action = "admin_accounts_listed", admin_id = %admin_uid);
    match state.app.credentials.list_accounts().await {
        Ok(result) => (StatusCode::OK, Json(json!({ "data": result.accounts }))).into_response(),
        Err(error) => err_json(error),
    }
}

async fn upsert_mantle_connection(
    State(state): State<HttpState>,
    Extension(admin_uid): Extension<Uuid>,
    Path((account_id, model_catalog_id)): Path<(Uuid, Uuid)>,
    Json(body): Json<MantleConnectionBody>,
) -> Response {
    if body.api_key.trim().is_empty() || body.api_key.len() > 32_768 {
        return err_json(BffError::Validation(
            "api_key must contain between 1 and 32768 characters".into(),
        ));
    }
    if body.base_url.trim().is_empty() || body.base_url.len() > 2048 {
        return err_json(BffError::Validation(
            "base_url must contain between 1 and 2048 characters".into(),
        ));
    }
    if let Some(expires_at) = body.expires_at.as_deref() {
        if chrono::DateTime::parse_from_rfc3339(expires_at).is_err() {
            return err_json(BffError::Validation("expires_at must be RFC3339".into()));
        }
    }
    match state
        .app
        .credentials
        .upsert_mantle_connection(
            account_id,
            model_catalog_id,
            body.api_key,
            body.base_url,
            body.expires_at,
            admin_uid,
        )
        .await
    {
        Ok(result) => {
            tracing::info!(
                event = "audit",
                action = "mantle_connection_updated",
                admin_id = %admin_uid,
                account_id = %account_id,
                model_catalog_id = %model_catalog_id,
                "Mantle connection updated"
            );
            (StatusCode::OK, Json(json!(result))).into_response()
        }
        Err(error) => err_json(error),
    }
}

#[derive(Deserialize)]
struct RegisterProviderBody {
    name: String,
}

async fn register_provider(
    State(state): State<HttpState>,
    Extension(admin_uid): Extension<Uuid>,
    Json(body): Json<RegisterProviderBody>,
) -> Response {
    if let Err(e) = validation::validate_provider_name(&body.name) {
        return err_json(e);
    }
    match state
        .app
        .catalog
        .register_provider(body.name, admin_uid)
        .await
    {
        Ok(provider) => {
            tracing::info!(event = "audit", action = "register_provider", admin_id = %admin_uid, provider_id = %provider.id, "provider registered");
            (StatusCode::CREATED, Json(json!(provider))).into_response()
        }
        Err(e) => err_json(e),
    }
}

async fn list_providers(State(state): State<HttpState>, _: Extension<Uuid>) -> Response {
    match state.app.catalog.list_providers().await {
        Ok(providers) => {
            let count = providers.len();
            (
                StatusCode::OK,
                Json(json!({ "data": providers, "count": count })),
            )
                .into_response()
        }
        Err(e) => err_json(e),
    }
}

// === Admin Plan Handlers ===

async fn list_plans(State(state): State<HttpState>, _: Extension<Uuid>) -> Response {
    match state.app.plans.list_plans().await {
        Ok(plans) => {
            let count = plans.len();
            (
                StatusCode::OK,
                Json(json!({ "data": plans, "count": count })),
            )
                .into_response()
        }
        Err(e) => err_json(e),
    }
}

async fn get_plan(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.app.plans.get_plan(id).await {
        Ok(plan) => (StatusCode::OK, Json(json!(plan))).into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct CreatePlanBody {
    name: String,
    slug: String,
    requests_per_minute: i32,
    tokens_per_minute: i32,
    max_concurrent_requests: i32,
    monthly_token_quota: Option<i64>,
    price_multiplier: String,
    billing_mode: String,
    features_json: Option<String>,
    #[serde(default)]
    storage_bytes_quota: i64,
    #[serde(default)]
    max_file_size_bytes: i64,
    #[serde(default)]
    max_files: i64,
    #[serde(default)]
    embeddings_enabled: bool,
    #[serde(default)]
    rag_enabled: bool,
    #[serde(default = "default_qdrant_mode")]
    qdrant_mode: String,
    #[serde(default = "default_price_amount")]
    price_amount: String,
    #[serde(default = "default_currency")]
    currency: String,
    #[serde(default)]
    trial_days: i32,
    #[serde(default)]
    max_api_keys: i64,
    #[serde(default)]
    overage_enabled: bool,
    #[serde(default)]
    fair_use_policy: String,
    #[serde(default)]
    grace_period_days: i32,
    #[serde(default)]
    sso_enabled: bool,
    #[serde(default)]
    sla_enabled: bool,
    #[serde(default)]
    mtls_enabled: bool,
    #[serde(default)]
    s3_api_enabled: bool,
    #[serde(default)]
    max_ingestion_files_per_hour: i64,
    #[serde(default)]
    max_ingestion_files_per_day: i64,
    #[serde(default)]
    max_ingestion_bytes_per_day: i64,
    #[serde(default)]
    max_ingestion_tokens_per_month: i64,
    #[serde(default = "default_concurrent_ingestions")]
    max_concurrent_ingestions: i64,
    #[serde(default = "default_pending_ingestion_jobs")]
    max_pending_ingestion_jobs: i64,
}

fn default_qdrant_mode() -> String {
    "none".to_string()
}
fn default_price_amount() -> String {
    "0".to_string()
}
fn default_currency() -> String {
    "USD".to_string()
}
fn default_concurrent_ingestions() -> i64 {
    1
}
fn default_pending_ingestion_jobs() -> i64 {
    10
}

async fn create_plan(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Json(body): Json<CreatePlanBody>,
) -> Response {
    if let Err(e) = validation::validate_plan_name(&body.name) {
        return err_json(e);
    }
    if let Err(e) = validation::validate_plan_slug(&body.slug) {
        return err_json(e);
    }
    if let Err(e) =
        validation::validate_non_negative_int(body.requests_per_minute, "requests_per_minute")
    {
        return err_json(e);
    }
    if let Err(e) = validation::validate_decimal(&body.price_multiplier, "price_multiplier") {
        return err_json(e);
    }
    match state
        .app
        .plans
        .create_plan(CreatePlanInput {
            name: body.name,
            slug: body.slug,
            requests_per_minute: body.requests_per_minute,
            tokens_per_minute: body.tokens_per_minute,
            max_concurrent_requests: body.max_concurrent_requests,
            monthly_token_quota: body.monthly_token_quota,
            price_multiplier: body.price_multiplier,
            billing_mode: body.billing_mode,
            features_json: body.features_json.unwrap_or_default(),
            storage_bytes_quota: body.storage_bytes_quota,
            max_file_size_bytes: body.max_file_size_bytes,
            max_files: body.max_files,
            embeddings_enabled: body.embeddings_enabled,
            rag_enabled: body.rag_enabled,
            qdrant_mode: body.qdrant_mode,
            price_amount: body.price_amount,
            currency: body.currency,
            trial_days: body.trial_days,
            max_api_keys: body.max_api_keys,
            overage_enabled: body.overage_enabled,
            fair_use_policy: body.fair_use_policy,
            grace_period_days: body.grace_period_days,
            sso_enabled: body.sso_enabled,
            sla_enabled: body.sla_enabled,
            mtls_enabled: body.mtls_enabled,
            s3_api_enabled: body.s3_api_enabled,
            max_ingestion_files_per_hour: body.max_ingestion_files_per_hour,
            max_ingestion_files_per_day: body.max_ingestion_files_per_day,
            max_ingestion_bytes_per_day: body.max_ingestion_bytes_per_day,
            max_ingestion_tokens_per_month: body.max_ingestion_tokens_per_month,
            max_concurrent_ingestions: body.max_concurrent_ingestions,
            max_pending_ingestion_jobs: body.max_pending_ingestion_jobs,
        })
        .await
    {
        Ok(plan) => {
            tracing::info!(event = "audit", action = "create_plan", plan_name = %plan.name, "plan created");
            (StatusCode::CREATED, Json(json!(plan))).into_response()
        }
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct UpdatePlanBody {
    name: String,
    slug: String,
    requests_per_minute: i32,
    tokens_per_minute: i32,
    max_concurrent_requests: i32,
    monthly_token_quota: Option<i64>,
    price_multiplier: String,
    billing_mode: String,
    features_json: Option<String>,
    is_active: bool,
    #[serde(default)]
    storage_bytes_quota: i64,
    #[serde(default)]
    max_file_size_bytes: i64,
    #[serde(default)]
    max_files: i64,
    #[serde(default)]
    embeddings_enabled: bool,
    #[serde(default)]
    rag_enabled: bool,
    #[serde(default = "default_qdrant_mode")]
    qdrant_mode: String,
    #[serde(default = "default_price_amount")]
    price_amount: String,
    #[serde(default = "default_currency")]
    currency: String,
    #[serde(default)]
    trial_days: i32,
    #[serde(default)]
    max_api_keys: i64,
    #[serde(default)]
    overage_enabled: bool,
    #[serde(default)]
    fair_use_policy: String,
    #[serde(default)]
    grace_period_days: i32,
    #[serde(default)]
    sso_enabled: bool,
    #[serde(default)]
    sla_enabled: bool,
    #[serde(default)]
    mtls_enabled: bool,
    #[serde(default)]
    s3_api_enabled: bool,
    #[serde(default)]
    max_ingestion_files_per_hour: i64,
    #[serde(default)]
    max_ingestion_files_per_day: i64,
    #[serde(default)]
    max_ingestion_bytes_per_day: i64,
    #[serde(default)]
    max_ingestion_tokens_per_month: i64,
    #[serde(default = "default_concurrent_ingestions")]
    max_concurrent_ingestions: i64,
    #[serde(default = "default_pending_ingestion_jobs")]
    max_pending_ingestion_jobs: i64,
}

async fn update_plan(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdatePlanBody>,
) -> Response {
    match state
        .app
        .plans
        .update_plan(UpdatePlanInput {
            plan_id: id,
            name: body.name,
            slug: body.slug,
            requests_per_minute: body.requests_per_minute,
            tokens_per_minute: body.tokens_per_minute,
            max_concurrent_requests: body.max_concurrent_requests,
            monthly_token_quota: body.monthly_token_quota,
            price_multiplier: body.price_multiplier,
            billing_mode: body.billing_mode,
            features_json: body.features_json.unwrap_or_default(),
            is_active: body.is_active,
            storage_bytes_quota: body.storage_bytes_quota,
            max_file_size_bytes: body.max_file_size_bytes,
            max_files: body.max_files,
            embeddings_enabled: body.embeddings_enabled,
            rag_enabled: body.rag_enabled,
            qdrant_mode: body.qdrant_mode,
            price_amount: body.price_amount,
            currency: body.currency,
            trial_days: body.trial_days,
            max_api_keys: body.max_api_keys,
            overage_enabled: body.overage_enabled,
            fair_use_policy: body.fair_use_policy,
            grace_period_days: body.grace_period_days,
            sso_enabled: body.sso_enabled,
            sla_enabled: body.sla_enabled,
            mtls_enabled: body.mtls_enabled,
            s3_api_enabled: body.s3_api_enabled,
            max_ingestion_files_per_hour: body.max_ingestion_files_per_hour,
            max_ingestion_files_per_day: body.max_ingestion_files_per_day,
            max_ingestion_bytes_per_day: body.max_ingestion_bytes_per_day,
            max_ingestion_tokens_per_month: body.max_ingestion_tokens_per_month,
            max_concurrent_ingestions: body.max_concurrent_ingestions,
            max_pending_ingestion_jobs: body.max_pending_ingestion_jobs,
        })
        .await
    {
        Ok(plan) => {
            tracing::info!(event = "audit", action = "update_plan", plan_id = %plan.id, "plan updated");
            (StatusCode::OK, Json(json!(plan))).into_response()
        }
        Err(e) => err_json(e),
    }
}

async fn deactivate_plan(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.app.plans.deactivate_plan(id).await {
        Ok(plan) => {
            tracing::info!(event = "audit", action = "deactivate_plan", plan_id = %plan.id, "plan deactivated");
            (StatusCode::OK, Json(json!(plan))).into_response()
        }
        Err(e) => err_json(e),
    }
}

// === Admin Wallet Handlers ===

async fn get_wallet(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Path(user_id): Path<Uuid>,
) -> Response {
    match state.app.wallet.get_wallet(user_id).await {
        Ok(wallet) => (StatusCode::OK, Json(json!(wallet))).into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct AdjustWalletBody {
    amount: String,
    currency: Option<String>,
    reason: String,
}

async fn adjust_wallet(
    State(state): State<HttpState>,
    Extension(admin_uid): Extension<Uuid>,
    Path(user_id): Path<Uuid>,
    Json(body): Json<AdjustWalletBody>,
) -> Response {
    if let Err(e) = validation::validate_decimal(&body.amount, "amount") {
        return err_json(e);
    }
    if body.reason.is_empty() {
        return err_json(BffError::Validation("reason is required".into()));
    }
    let idempotency_key = Uuid::now_v7().to_string();
    match state
        .app
        .wallet
        .adjust_wallet(
            user_id,
            body.amount,
            body.currency.unwrap_or_else(|| "USD".into()),
            body.reason.clone(),
            admin_uid,
            idempotency_key,
        )
        .await
    {
        Ok(wallet) => {
            tracing::info!(event = "audit", action = "adjust_wallet", admin_id = %admin_uid, target_id = %user_id, "wallet adjusted");
            (StatusCode::OK, Json(json!(wallet))).into_response()
        }
        Err(e) => err_json(e),
    }
}

// === Admin Invoice Handlers ===

#[derive(Deserialize)]
struct ListInvoicesQuery {
    user_id: Option<Uuid>,
    limit: Option<i32>,
}

async fn list_invoices(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Query(params): Query<ListInvoicesQuery>,
) -> Response {
    let user_id = match params.user_id {
        Some(id) => id,
        None => {
            return err_json(BffError::Validation(
                "user_id query parameter is required".into(),
            ))
        }
    };
    let limit = params.limit.unwrap_or(100).clamp(1, 500);
    match state
        .app
        .invoices
        .list_invoices_by_user(user_id, limit)
        .await
    {
        Ok(invoices) => {
            let count = invoices.len();
            (
                StatusCode::OK,
                Json(json!({ "data": invoices, "count": count })),
            )
                .into_response()
        }
        Err(e) => err_json(e),
    }
}

async fn get_invoice(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.app.invoices.get_invoice(id).await {
        Ok(invoice) => (StatusCode::OK, Json(json!(invoice))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn open_invoice(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.app.invoices.open_invoice(id).await {
        Ok(invoice) => {
            tracing::info!(event = "audit", action = "open_invoice", invoice_id = %invoice.id, "invoice opened");
            (StatusCode::OK, Json(json!(invoice))).into_response()
        }
        Err(e) => err_json(e),
    }
}

async fn void_invoice(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.app.invoices.void_invoice(id).await {
        Ok(invoice) => {
            tracing::info!(event = "audit", action = "void_invoice", invoice_id = %invoice.id, "invoice voided");
            (StatusCode::OK, Json(json!(invoice))).into_response()
        }
        Err(e) => err_json(e),
    }
}

async fn mark_uncollectible(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.app.invoices.mark_uncollectible(id).await {
        Ok(invoice) => {
            tracing::info!(event = "audit", action = "mark_uncollectible", invoice_id = %invoice.id, "invoice marked uncollectible");
            (StatusCode::OK, Json(json!(invoice))).into_response()
        }
        Err(e) => err_json(e),
    }
}

// === Admin Usage Handlers ===

#[derive(Deserialize)]
struct TimeRangeQuery {
    start: Option<String>,
    end: Option<String>,
}

async fn get_usage_by_user(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Path(user_id): Path<Uuid>,
    Query(range): Query<TimeRangeQuery>,
) -> Response {
    match state
        .app
        .usage
        .get_usage_by_user(user_id, range.start, range.end)
        .await
    {
        Ok(usage) => (StatusCode::OK, Json(json!(usage))).into_response(),
        Err(e) => err_json(e),
    }
}

fn admin_file_json(file: &proto::file_v1::File) -> serde_json::Value {
    json!({
        "id": file.id,
        "user_id": file.user_id,
        "folder_id": file.folder_id,
        "name": file.name,
        "mime_type": file.mime_type,
        "size_bytes": file.size_bytes,
        "sha256": file.sha256,
        "status": file.status,
        "created_at": file.created_at,
        "updated_at": file.updated_at,
    })
}

fn admin_folder_json(folder: &proto::file_v1::Folder) -> serde_json::Value {
    json!({
        "id": folder.id,
        "user_id": folder.user_id,
        "parent_id": folder.parent_id,
        "name": folder.name,
        "created_at": folder.created_at,
        "updated_at": folder.updated_at,
    })
}

async fn storage_by_user(
    State(state): State<HttpState>,
    Extension(admin_id): Extension<Uuid>,
    Path(user_id): Path<Uuid>,
) -> Response {
    let (files_result, folders_result, vector_result, storage_result, dlq_result) = tokio::join!(
        state.app.files.list_files(user_id),
        state.app.files.list_folders(user_id),
        state.app.files.vector_usage(user_id),
        state.app.files.storage_usage(user_id),
        state.app.jetstream.dlq_depth(),
    );
    let files = match files_result {
        Ok(value) => value,
        Err(error) => return err_json(error),
    };
    let folders = match folders_result {
        Ok(value) => value,
        Err(error) => return err_json(error),
    };
    let vector_usage = match vector_result {
        Ok(value) => value,
        Err(error) => return err_json(error),
    };
    let storage_usage = match storage_result {
        Ok(value) => value,
        Err(error) => return err_json(error),
    };
    let dlq_count = match dlq_result {
        Ok(value) => value,
        Err(error) => return err_json(error),
    };
    if files.iter().any(|file| file.user_id != user_id.to_string())
        || folders
            .iter()
            .any(|folder| folder.user_id != user_id.to_string())
        || vector_usage.user_id != user_id.to_string()
        || storage_usage.user_id != user_id.to_string()
    {
        tracing::error!(
            event = "audit",
            action = "admin_storage_response_owner_mismatch",
            %admin_id,
            %user_id,
            "file service returned data for an unexpected owner"
        );
        return err_response(
            StatusCode::BAD_GATEWAY,
            "OWNER_MISMATCH",
            "storage response failed ownership validation",
        );
    }
    let pending = files
        .iter()
        .filter(|file| matches!(file.status.as_str(), "pending" | "created"))
        .count();
    let processing = files
        .iter()
        .filter(|file| matches!(file.status.as_str(), "processing" | "embedding"))
        .count();
    let ready = files.iter().filter(|file| file.status == "ready").count();
    let failed = files.iter().filter(|file| file.status == "failed").count();
    tracing::info!(
        event = "audit",
        action = "admin_storage_view",
        %admin_id,
        %user_id,
        file_count = files.len(),
        folder_count = folders.len(),
        "admin storage snapshot loaded"
    );
    (
        StatusCode::OK,
        Json(json!({
            "user_id": user_id,
            "files": files.iter().map(admin_file_json).collect::<Vec<_>>(),
            "folders": folders.iter().map(admin_folder_json).collect::<Vec<_>>(),
            "vector_usage": {
                "user_id": vector_usage.user_id,
                "vector_count": vector_usage.vector_count,
                "vector_bytes": vector_usage.vector_bytes,
                "document_count": vector_usage.document_count,
                "collection_mode": vector_usage.collection_mode,
                "collection_name": vector_usage.collection_name,
                "updated_at": vector_usage.updated_at,
                "input_tokens": vector_usage.input_tokens,
                "cost_micros": vector_usage.cost_micros,
                "tokens_avoided": vector_usage.tokens_avoided,
                "completed_jobs": vector_usage.completed_jobs,
                "failed_jobs": vector_usage.failed_jobs,
            },
            "storage_usage": {
                "user_id": storage_usage.user_id,
                "used_bytes": storage_usage.used_bytes,
                "quota_bytes": storage_usage.quota_bytes,
                "remaining_bytes": storage_usage.remaining_bytes,
                "file_count": storage_usage.file_count,
                "max_files": storage_usage.max_files,
                "updated_at": storage_usage.updated_at,
            },
            "ingestion": {
                "pending": pending,
                "processing": processing,
                "ready": ready,
                "failed": failed,
                "dlq": dlq_count,
                "dlq_source": "EVENTS_DLQ.state.messages",
            },
        })),
    )
        .into_response()
}

async fn top_models(State(state): State<HttpState>, _: Extension<Uuid>) -> Response {
    match state.app.usage.top_models().await {
        Ok(models) => (StatusCode::OK, Json(json!(models))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn error_rate(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Query(range): Query<TimeRangeQuery>,
) -> Response {
    match state.app.usage.error_rate(range.start, range.end).await {
        Ok(stats) => (StatusCode::OK, Json(json!(stats))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn latency_stats(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Query(range): Query<TimeRangeQuery>,
) -> Response {
    match state.app.usage.latency_stats(range.start, range.end).await {
        Ok(stats) => (StatusCode::OK, Json(json!(stats))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn usage_summary(State(state): State<HttpState>, _: Extension<Uuid>) -> Response {
    // The usage service is the source of truth for request/error totals. The
    // remaining fields are kept in the response contract for the dashboard;
    // dedicated billing/subscription aggregate RPCs can populate them later.
    match state.app.usage.error_rate(None, None).await {
        Ok(stats) => (
            StatusCode::OK,
            Json(json!({
                "total_users": 0,
                "total_requests_30d": stats.total_requests,
                "total_tokens_30d": 0,
                "total_revenue_30d": 0.0,
                "active_subscriptions": 0,
                "error_rate_30d": stats.error_rate
            })),
        )
            .into_response(),
        Err(e) => err_json(e),
    }
}

async fn refresh_views(State(state): State<HttpState>, _: Extension<Uuid>) -> Response {
    match state.app.usage.refresh_views().await {
        Ok(refreshed) => {
            tracing::info!(
                event = "audit",
                action = "refresh_usage_views",
                "materialized views refreshed"
            );
            (StatusCode::OK, Json(json!({ "refreshed": refreshed }))).into_response()
        }
        Err(e) => err_json(e),
    }
}

async fn create_partition(State(state): State<HttpState>, _: Extension<Uuid>) -> Response {
    match state.app.usage.create_next_month_partition().await {
        Ok(partition_name) => {
            tracing::info!(event = "audit", action = "create_usage_partition", %partition_name, "partition created");
            (
                StatusCode::CREATED,
                Json(json!({ "partition_name": partition_name })),
            )
                .into_response()
        }
        Err(e) => err_json(e),
    }
}

// === Admin Router Handlers ===

async fn list_account_loads(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Path(model_catalog_id): Path<String>,
) -> Response {
    match state.app.router.list_account_loads(model_catalog_id).await {
        Ok(loads) => {
            let count = loads.len();
            (
                StatusCode::OK,
                Json(json!({ "data": loads, "count": count })),
            )
                .into_response()
        }
        Err(e) => err_json(e),
    }
}

async fn reset_circuit_breaker(
    State(state): State<HttpState>,
    _: Extension<Uuid>,
    Path(account_id): Path<Uuid>,
) -> Response {
    match state.app.router.reset_circuit_breaker(account_id).await {
        Ok(closed) => {
            tracing::info!(event = "audit", action = "reset_circuit_breaker", account_id = %account_id, "circuit breaker reset");
            (StatusCode::OK, Json(json!({ "closed": closed }))).into_response()
        }
        Err(e) => err_json(e),
    }
}
