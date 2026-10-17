use crate::application::service::BffApplication;
use crate::domain::errors::BffError;
use crate::domain::validation;
use crate::infrastructure::clients::LoginResult;
use crate::infrastructure::rate_limiter::RedisRateLimiter;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode};
use axum::middleware::{from_fn, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
use axum::{Extension, Json, Router};
use base64::Engine;
use metrics_exporter_prometheus::PrometheusHandle;
use rust_decimal::Decimal;
use serde::Deserialize;
use serde_json::json;
use std::collections::HashMap;
use std::str::FromStr;
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
                    let uid = Uuid::parse_str(&result.user.id).unwrap_or(Uuid::nil());
                    tracing::info!(%uid, "silent session refresh");
                    (uid, Some(result.access_token), Some(result.refresh_token))
                }
                Err(_) => match app.auth.validate_token(&access_token).await {
                    Ok(s) => (s.user_id, None, None),
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
        || path == "/api/v1/auth/register"
        || path == "/api/v1/auth/login"
        || path == "/api/v1/auth/refresh"
        || path == "/api/v1/auth/verify-email"
        || path == "/api/v1/plans"
        || path.starts_with("/api/v1/auth/request-password-reset")
        || path.starts_with("/api/v1/auth/reset-password")
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
        .route("/api/v1/auth/register", post(register))
        .route("/api/v1/auth/login", post(login))
        .route("/api/v1/auth/refresh", post(refresh))
        .route("/api/v1/auth/verify-email", post(verify_email))
        .route("/api/v1/auth/logout", post(logout))
        .route("/api/v1/auth/logout-all", post(logout_all))
        .route("/api/v1/auth/sessions", get(list_sessions))
        .route("/api/v1/auth/sessions/{id}", delete(revoke_session))
        .route("/api/v1/auth/change-password", post(change_password))
        .route("/api/v1/auth/enable-mfa", post(enable_mfa))
        .route("/api/v1/auth/confirm-mfa", post(confirm_mfa))
        .route("/api/v1/auth/disable-mfa", post(disable_mfa))
        .route("/api/v1/me", get(get_profile).patch(update_profile))
        .route("/api/v1/api-keys", get(list_keys).post(create_key))
        .route(
            "/api/v1/api-keys/{id}",
            get(get_key).patch(update_key).delete(revoke_key),
        )
        .route("/api/v1/api-keys/{id}/rotate", post(rotate_key))
        .route("/api/v1/files", get(list_files).post(create_file))
        .route("/api/v1/files/{id}/upload-url", post(create_upload_url))
        .route("/api/v1/files/{id}/download-url", post(download_file))
        .route("/api/v1/files/{id}/confirm", post(confirm_file))
        .route("/api/v1/files/{id}/vectors", delete(delete_file_vectors))
        .route(
            "/api/v1/files/{id}/vectors/reindex",
            post(reindex_file_vectors),
        )
        .route("/api/v1/files/{id}", put(update_file).delete(delete_file))
        .route("/api/v1/vectors/search", post(search_vectors))
        .route("/api/v1/vectors/usage", get(vector_usage))
        .route("/api/v1/storage/usage", get(storage_usage))
        .route("/api/v1/folders", get(list_folders).post(create_folder))
        .route(
            "/api/v1/folders/{id}",
            put(update_folder).delete(delete_folder),
        )
        .route("/api/v1/plans", get(list_plans))
        .route("/api/v1/plans/{id}", get(get_plan))
        .route("/api/v1/legal/accept", post(accept_legal))
        .route(
            "/api/v1/subscription",
            get(get_subscription).post(create_subscription),
        )
        .route(
            "/api/v1/subscription/paypal/confirm",
            post(confirm_paypal_subscription),
        )
        .route(
            "/api/v1/subscription/paypal/vault/confirm",
            post(confirm_paypal_vault_subscription),
        )
        .route("/api/v1/subscription/cancel", post(cancel_subscription))
        .route("/api/v1/wallet", get(get_wallet))
        .route("/api/v1/payment-methods", get(list_payment_methods))
        .route(
            "/api/v1/payment-methods/{id}/default",
            put(set_default_payment_method),
        )
        .route("/api/v1/top-up", post(create_top_up))
        .route("/api/v1/invoices", get(list_invoices))
        .route("/api/v1/invoices/{id}", get(get_invoice))
        .route("/api/v1/models", get(list_models))
        .route("/api/v1/models/{id}", get(get_model))
        .route("/api/v1/usage", get(current_usage))
        .route("/api/v1/usage/top-models", get(top_models))
        .route("/api/v1/usage/latency", get(latency_stats))
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
        ("keys", &state.app.keys.config),
        ("plans", &state.app.plans.config),
        ("catalog", &state.app.catalog.config),
        ("wallet", &state.app.wallet.config),
        ("invoices", &state.app.invoices.config),
        ("payments", &state.app.payments.config),
        ("usage", &state.app.usage.config),
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
    // A response may set both the HttpOnly session cookie and the CSRF cookie.
    // `insert` would replace an earlier Set-Cookie header, silently dropping
    // the session cookie during login. Set-Cookie is intentionally append-only.
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

#[derive(Deserialize)]
struct RegisterBody {
    email: String,
    password: String,
    display_name: Option<String>,
}

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

async fn register(State(state): State<HttpState>, Json(body): Json<RegisterBody>) -> Response {
    if let Err(e) = validation::validate_email(&body.email) {
        return err_json(e);
    }
    if let Err(e) = validation::validate_password(&body.password) {
        return err_json(e);
    }
    if let Some(ref name) = body.display_name {
        if let Err(e) = validation::validate_string_length(name, 100, "display_name") {
            return err_json(e);
        }
    }
    match state
        .app
        .auth
        .register(body.email, body.password, body.display_name)
        .await
    {
        Ok(user) => {
            tracing::info!(event = "audit", action = "register", email = %user.email, "user registered");
            (StatusCode::CREATED, Json(json!(user))).into_response()
        }
        Err(e) => err_json(e),
    }
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
    let has_mfa = body.mfa_code.is_some();
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
            tracing::info!(event = "audit", action = "login", email = %result.user.email, mfa = has_mfa, "user logged in");
            login_response(result, state.cookie_secure, state.cookie_domain.as_deref())
        }
        Err(e) => {
            tracing::warn!(event = "audit", action = "login", email = %email, error = %e, "login failed");
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

#[derive(Deserialize)]
struct VerifyEmailBody {
    token: String,
}

async fn verify_email(
    State(state): State<HttpState>,
    Json(body): Json<VerifyEmailBody>,
) -> Response {
    if body.token.is_empty() {
        return err_json(BffError::Validation("token is required".into()));
    }
    match state.app.auth.verify_email(body.token).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "success": true }))).into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct LogoutBody {
    refresh_token: String,
}

async fn logout(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Json(body): Json<LogoutBody>,
) -> Response {
    if let Err(e) = validation::validate_string_length(&body.refresh_token, 1024, "refresh_token") {
        return err_json(e);
    }
    let mut response = match state.app.auth.logout(uid, body.refresh_token).await {
        Ok(_) => {
            tracing::info!(event = "audit", action = "logout", user_id = %uid, "user logged out");
            (StatusCode::OK, Json(json!({ "success": true }))).into_response()
        }
        Err(e) => return err_json(e),
    };
    clear_session_cookie(
        state.cookie_secure,
        state.cookie_domain.as_deref(),
        &mut response,
    );
    response
}

async fn logout_all(State(state): State<HttpState>, Extension(uid): Extension<Uuid>) -> Response {
    let mut response = match state.app.auth.logout_all(uid).await {
        Ok(_) => {
            tracing::info!(event = "audit", action = "logout_all", user_id = %uid, "all sessions terminated");
            (StatusCode::OK, Json(json!({ "success": true }))).into_response()
        }
        Err(e) => return err_json(e),
    };
    clear_session_cookie(
        state.cookie_secure,
        state.cookie_domain.as_deref(),
        &mut response,
    );
    response
}

async fn list_sessions(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
) -> Response {
    match state.app.auth.list_sessions(uid).await {
        Ok(sessions) => (StatusCode::OK, Json(json!({ "sessions": sessions }))).into_response(),
        Err(error) => err_json(error),
    }
}

async fn revoke_session(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Path(session_id): Path<String>,
) -> Response {
    let session_id = match Uuid::parse_str(&session_id) {
        Ok(value) => value,
        Err(error) => {
            return err_json(BffError::Validation(format!("invalid session id: {error}")));
        }
    };
    match state.app.auth.revoke_session(uid, session_id).await {
        Ok(()) => (StatusCode::OK, Json(json!({ "success": true }))).into_response(),
        Err(error) => err_json(error),
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
    match state
        .app
        .auth
        .update_profile(uid, body.display_name, body.avatar_url)
        .await
    {
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
    match state
        .app
        .auth
        .change_password(uid, body.old_password, body.new_password)
        .await
    {
        Ok(_) => {
            tracing::info!(event = "audit", action = "change_password", user_id = %uid, "password changed");
            (StatusCode::OK, Json(json!({ "success": true }))).into_response()
        }
        Err(e) => err_json(e),
    }
}

async fn enable_mfa(State(state): State<HttpState>, Extension(uid): Extension<Uuid>) -> Response {
    match state.app.auth.enable_mfa(uid).await {
        Ok(uri) => (StatusCode::OK, Json(json!({ "otpauth_uri": uri }))).into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct MfaCodeBody {
    code: String,
}

async fn confirm_mfa(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Json(body): Json<MfaCodeBody>,
) -> Response {
    if let Err(e) = validation::validate_mfa_code(&body.code) {
        return err_json(e);
    }
    match state.app.auth.confirm_mfa(uid, body.code).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "success": true }))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn disable_mfa(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Json(body): Json<MfaCodeBody>,
) -> Response {
    if let Err(e) = validation::validate_mfa_code(&body.code) {
        return err_json(e);
    }
    match state.app.auth.disable_mfa(uid, body.code).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "success": true }))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn list_keys(State(state): State<HttpState>, Extension(uid): Extension<Uuid>) -> Response {
    match state.app.keys.list_keys(uid).await {
        Ok(keys) => {
            let count = keys.len();
            (
                StatusCode::OK,
                Json(json!({ "data": keys, "count": count })),
            )
                .into_response()
        }
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct CreateKeyBody {
    name: String,
    #[serde(default)]
    scopes: Vec<String>,
}

fn validate_key_scopes(scopes: &[String]) -> Result<Vec<String>, BffError> {
    let allowed = [
        "inference:read",
        "inference:write",
        "keys:manage",
        "billing:read",
        "files:read",
        "files:write",
        "files:delete",
    ];
    let mut values = if scopes.is_empty() {
        vec!["inference:write".to_string()]
    } else {
        scopes.to_vec()
    };
    values.sort();
    values.dedup();
    if values
        .iter()
        .any(|scope| !allowed.contains(&scope.as_str()))
    {
        return Err(BffError::Validation("invalid api key scope".into()));
    }
    Ok(values)
}

async fn create_key(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Json(body): Json<CreateKeyBody>,
) -> Response {
    if body.name.trim().is_empty() {
        return err_json(BffError::Validation("name is required".into()));
    }
    if let Err(e) = validation::validate_string_length(&body.name, 100, "name") {
        return err_json(e);
    }
    let scopes = match validate_key_scopes(&body.scopes) {
        Ok(scopes) => scopes,
        Err(error) => return err_json(error),
    };
    match state.app.keys.create_key(uid, body.name, scopes).await {
        Ok(resp) => {
            let key_id = resp
                .key
                .as_ref()
                .map(|k| &k.id)
                .cloned()
                .unwrap_or_default();
            tracing::info!(event = "audit", action = "create_key", user_id = %uid, key_id = %key_id, "api key created");
            (StatusCode::CREATED, Json(json!(resp))).into_response()
        }
        Err(e) => err_json(e),
    }
}

async fn get_key(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.app.keys.get_key(id, uid).await {
        Ok(key) => (StatusCode::OK, Json(json!(key))).into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct UpdateKeyBody {
    name: String,
    #[serde(default)]
    scopes: Vec<String>,
}

async fn update_key(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateKeyBody>,
) -> Response {
    if let Err(error) = validation::validate_string_length(&body.name, 100, "name") {
        return err_json(error);
    }
    let scopes = match validate_key_scopes(&body.scopes) {
        Ok(scopes) => scopes,
        Err(error) => return err_json(error),
    };
    match state.app.keys.update_key(uid, id, body.name, scopes).await {
        Ok(key) => (StatusCode::OK, Json(json!(key))).into_response(),
        Err(error) => err_json(error),
    }
}

#[derive(Deserialize, Default)]
struct RotateKeyBody {
    scopes: Option<Vec<String>>,
}

async fn rotate_key(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
    Json(body): Json<RotateKeyBody>,
) -> Response {
    let scopes = match body.scopes.as_deref().map(validate_key_scopes).transpose() {
        Ok(scopes) => scopes,
        Err(error) => return err_json(error),
    };
    match state.app.keys.rotate_key(uid, id, scopes).await {
        Ok(response) => (StatusCode::OK, Json(json!(response))).into_response(),
        Err(error) => err_json(error),
    }
}

async fn revoke_key(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.app.keys.revoke_key(uid, id).await {
        Ok(_) => {
            tracing::info!(event = "audit", action = "revoke_key", user_id = %uid, key_id = %id, "api key revoked");
            (StatusCode::OK, Json(json!({ "success": true }))).into_response()
        }
        Err(e) => err_json(e),
    }
}

fn file_json(file: &proto::file_v1::File) -> serde_json::Value {
    json!({"id": file.id, "user_id": file.user_id, "folder_id": file.folder_id, "name": file.name, "mime_type": file.mime_type, "size_bytes": file.size_bytes, "sha256": file.sha256, "status": file.status, "object_key": file.object_key, "created_at": file.created_at, "updated_at": file.updated_at})
}

async fn list_files(State(state): State<HttpState>, Extension(uid): Extension<Uuid>) -> Response {
    match state.app.files.list_files(uid, None).await {
        Ok(files) => (
            StatusCode::OK,
            Json(json!({"data": files.iter().map(file_json).collect::<Vec<_>>()})),
        )
            .into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct CreateFileBody {
    name: String,
    mime_type: String,
    size_bytes: i64,
    sha256: String,
    idempotency_key: String,
    folder_id: Option<String>,
}

async fn create_file(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Json(body): Json<CreateFileBody>,
) -> Response {
    let request = proto::file_v1::CreateFileRequest {
        user_id: uid.to_string(),
        folder_id: body.folder_id,
        name: body.name,
        mime_type: body.mime_type,
        size_bytes: body.size_bytes,
        sha256: body.sha256,
        idempotency_key: body.idempotency_key,
        object_key: None,
    };
    match state.app.files.create_file(request).await {
        Ok(file) => (StatusCode::CREATED, Json(file_json(&file))).into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct UploadUrlBody {
    expires_seconds: Option<i32>,
}

async fn create_upload_url(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
    Json(body): Json<UploadUrlBody>,
) -> Response {
    match state.app.files.create_upload_url(uid, id, body.expires_seconds).await {
        Ok(result) => (StatusCode::OK, Json(json!({"file": result.file.as_ref().map(file_json), "upload_url": result.upload_url, "required_headers": result.required_headers, "expires_at": result.expires_at}))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn confirm_file(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.app.files.confirm_file(uid, id).await {
        Ok(file) => (StatusCode::OK, Json(file_json(&file))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn delete_file(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state
        .app
        .files
        .delete_file(uid, id, "user_requested".into())
        .await
    {
        Ok(_) => (StatusCode::OK, Json(json!({"success": true}))).into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct VectorSearchBody {
    query: String,
    limit: Option<i32>,
    file_id: Option<Uuid>,
}

async fn search_vectors(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Json(body): Json<VectorSearchBody>,
) -> Response {
    let query = body.query.trim();
    if query.is_empty() || query.len() > 16_384 {
        return err_json(BffError::Validation(
            "query must contain between 1 and 16384 characters".into(),
        ));
    }
    let limit = body.limit.unwrap_or(10);
    if !(1..=100).contains(&limit) {
        return err_json(BffError::Validation(
            "limit must be between 1 and 100".into(),
        ));
    }
    match state
        .app
        .files
        .search_vectors(
            uid,
            query.to_owned(),
            limit,
            body.file_id.map(|id| id.to_string()),
        )
        .await
    {
        Ok(hits) => (
            StatusCode::OK,
            Json(json!({"data": hits, "count": hits.len()})),
        )
            .into_response(),
        Err(error) => err_json(error),
    }
}

async fn vector_usage(State(state): State<HttpState>, Extension(uid): Extension<Uuid>) -> Response {
    match state.app.files.vector_usage(uid).await {
        Ok(usage) => (StatusCode::OK, Json(json!(usage))).into_response(),
        Err(error) => err_json(error),
    }
}

async fn storage_usage(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
) -> Response {
    match state.app.files.storage_usage(uid).await {
        Ok(usage) => (StatusCode::OK, Json(json!(usage))).into_response(),
        Err(error) => err_json(error),
    }
}

async fn delete_file_vectors(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Path(file_id): Path<Uuid>,
) -> Response {
    match state
        .app
        .files
        .delete_file_vectors(uid, file_id, "user_requested_reindex".into())
        .await
    {
        Ok(deleted_vectors) => {
            tracing::info!(
                event = "audit",
                action = "delete_file_vectors",
                %uid,
                %file_id,
                deleted_vectors,
                "file vectors deleted"
            );
            (
                StatusCode::OK,
                Json(json!({"success":true,"deleted_vectors":deleted_vectors})),
            )
                .into_response()
        }
        Err(error) => err_json(error),
    }
}

async fn reindex_file_vectors(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Path(file_id): Path<Uuid>,
) -> Response {
    match state.app.files.reindex_file_vectors(uid, file_id).await {
        Ok(()) => {
            tracing::info!(
                event = "audit",
                action = "reindex_file_vectors",
                %uid,
                %file_id,
                "file vector reindex accepted"
            );
            (
                StatusCode::ACCEPTED,
                Json(json!({"accepted":true,"file_id":file_id})),
            )
                .into_response()
        }
        Err(error) => err_json(error),
    }
}

async fn download_file(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
    body: Option<Json<UploadUrlBody>>,
) -> Response {
    let expires_seconds = body.and_then(|Json(value)| value.expires_seconds);
    match state.app.files.download_url(uid, id, expires_seconds).await {
        Ok(result) => (
            StatusCode::OK,
            Json(json!({
                "file": result.file.as_ref().map(file_json),
                "download_url": result.download_url,
                "expires_at": result.expires_at
            })),
        )
            .into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct UpdateFileBody {
    name: Option<String>,
    folder_id: Option<String>,
}

async fn update_file(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateFileBody>,
) -> Response {
    if body.name.is_none() && body.folder_id.is_none() {
        return err_json(BffError::Validation("name or folder_id is required".into()));
    }
    match state
        .app
        .files
        .update_file(uid, id, body.name, body.folder_id)
        .await
    {
        Ok(file) => (StatusCode::OK, Json(file_json(&file))).into_response(),
        Err(e) => err_json(e),
    }
}

fn folder_json(folder: &proto::file_v1::Folder) -> serde_json::Value {
    json!({
        "id": folder.id,
        "user_id": folder.user_id,
        "parent_id": folder.parent_id,
        "name": folder.name,
        "created_at": folder.created_at,
        "updated_at": folder.updated_at
    })
}

#[derive(Deserialize)]
struct CreateFolderBody {
    name: String,
    parent_id: Option<String>,
    idempotency_key: String,
}

#[derive(Deserialize, Default)]
struct FolderListQuery {
    parent_id: Option<String>,
}

async fn list_folders(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Query(query): Query<FolderListQuery>,
) -> Response {
    match state.app.files.list_folders(uid, query.parent_id).await {
        Ok(folders) => (
            StatusCode::OK,
            Json(json!({
                "data": folders.iter().map(folder_json).collect::<Vec<_>>()
            })),
        )
            .into_response(),
        Err(e) => err_json(e),
    }
}

async fn create_folder(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Json(body): Json<CreateFolderBody>,
) -> Response {
    if body.name.trim().is_empty() || body.idempotency_key.trim().is_empty() {
        return err_json(BffError::Validation(
            "name and idempotency_key are required".into(),
        ));
    }
    match state
        .app
        .files
        .create_folder(uid, body.name, body.parent_id, body.idempotency_key)
        .await
    {
        Ok(folder) => (StatusCode::CREATED, Json(folder_json(&folder))).into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct UpdateFolderBody {
    name: Option<String>,
    parent_id: Option<String>,
}

async fn update_folder(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateFolderBody>,
) -> Response {
    if body.name.is_none() && body.parent_id.is_none() {
        return err_json(BffError::Validation("name or parent_id is required".into()));
    }
    match state
        .app
        .files
        .update_folder(uid, id, body.name, body.parent_id)
        .await
    {
        Ok(folder) => (StatusCode::OK, Json(folder_json(&folder))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn delete_folder(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state
        .app
        .files
        .delete_folder(uid, id, "user_requested".into())
        .await
    {
        Ok(_) => (StatusCode::OK, Json(json!({ "success": true }))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn list_plans(State(state): State<HttpState>) -> Response {
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

async fn get_plan(State(state): State<HttpState>, Path(id): Path<Uuid>) -> Response {
    match state.app.plans.get_plan(id).await {
        Ok(plan) => (StatusCode::OK, Json(json!(plan))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn get_subscription(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
) -> Response {
    match state.app.plans.get_subscription(uid).await {
        Ok(sub) => (StatusCode::OK, Json(json!(sub))).into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct CreateSubscriptionBody {
    plan_id: Uuid,
    #[serde(default)]
    provider: Option<String>,
    terms_version: String,
    privacy_version: String,
    #[serde(default)]
    return_url: String,
    #[serde(default)]
    cancel_url: String,
}

#[derive(Deserialize)]
struct AcceptLegalBody {
    terms_version: String,
    privacy_version: String,
    country: Option<String>,
}

async fn accept_legal(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    headers: axum::http::HeaderMap,
    Json(body): Json<AcceptLegalBody>,
) -> Response {
    if body.terms_version != "2026-07-24" || body.privacy_version != "2026-07-24" {
        return err_json(BffError::Validation(
            "unsupported legal document version".into(),
        ));
    }
    let ip = headers
        .get("cf-connecting-ip")
        .or_else(|| headers.get("x-forwarded-for"))
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    let ua = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    match state
        .app
        .plans
        .accept_terms(
            uid,
            body.terms_version,
            body.privacy_version,
            ip,
            ua,
            body.country.unwrap_or_default(),
        )
        .await
    {
        Ok(at) => (
            StatusCode::OK,
            Json(json!({"accepted": true, "accepted_at": at})),
        )
            .into_response(),
        Err(e) => err_json(e),
    }
}

async fn create_subscription(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Json(body): Json<CreateSubscriptionBody>,
) -> Response {
    if body.plan_id.is_nil() {
        return err_json(BffError::Validation("plan_id is required".into()));
    }
    let plan = match state.app.plans.get_plan(body.plan_id).await {
        Ok(p) => p,
        Err(e) => return err_json(e),
    };
    let amount = match Decimal::from_str(plan.price_amount.trim()) {
        Ok(value) if value >= Decimal::ZERO => value,
        Ok(_) => return err_json(BffError::Validation("plan price cannot be negative".into())),
        Err(_) => return err_json(BffError::Internal("plan has an invalid price".into())),
    };
    if amount.is_zero() {
        return match state
            .app
            .plans
            .create_subscription(uid, body.plan_id, body.terms_version, body.privacy_version)
            .await
        {
            Ok(sub) => (
                StatusCode::CREATED,
                Json(json!({"subscription": sub, "status": "trialing", "redirect_url": "/dashboard"})),
            )
                .into_response(),
            Err(e) => err_json(e),
        };
    }
    if let Some(provider) = body.provider.as_deref() {
        if !provider.eq_ignore_ascii_case("paypal") {
            return err_json(BffError::Validation(
                "only PayPal is currently available for paid plans".into(),
            ));
        }
    }
    let currency = if plan.currency.is_empty() {
        "USD".to_string()
    } else {
        plan.currency.clone()
    };
    if plan.trial_days > 0 {
        let return_url = if body.return_url.is_empty() {
            "/subscription/confirm?flow=vault".to_string()
        } else {
            format!(
                "{}{}flow=vault",
                body.return_url,
                if body.return_url.contains('?') {
                    '&'
                } else {
                    '?'
                }
            )
        };
        return match state
            .app
            .payments
            .create_paypal_vault_setup_token(uid, return_url, body.cancel_url.clone())
            .await
        {
            Ok(setup) => (
                StatusCode::ACCEPTED,
                Json(json!({
                    "redirect_url": setup.approval_url,
                    "plan_id": body.plan_id,
                    "flow": "paypal_vault_setup",
                    "status": setup.status,
                    "currency": currency,
                    "trial_days": plan.trial_days
                })),
            )
                .into_response(),
            Err(e) => err_json(e),
        };
    }
    match state
        .app
        .payments
        .create_subscription_order(
            uid,
            amount.to_string(),
            currency,
            body.plan_id,
            body.return_url,
            body.cancel_url,
        )
        .await
    {
        Ok(order) => (StatusCode::ACCEPTED, Json(json!({"redirect_url": order.approval_url, "order_id": order.provider_reference, "plan_id": body.plan_id, "status": order.status}))).into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct ConfirmPayPalSubscriptionBody {
    order_id: String,
    plan_id: Uuid,
    terms_version: String,
    privacy_version: String,
}

async fn confirm_paypal_subscription(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Json(body): Json<ConfirmPayPalSubscriptionBody>,
) -> Response {
    if body.plan_id.is_nil() || body.order_id.trim().is_empty() {
        return err_json(BffError::Validation(
            "order_id and plan_id are required".into(),
        ));
    }
    if let Err(e) = state
        .app
        .payments
        .capture_paypal_order(uid, body.order_id)
        .await
    {
        return err_json(e);
    }
    match state
        .app
        .plans
        .create_subscription(uid, body.plan_id, body.terms_version, body.privacy_version)
        .await
    {
        Ok(sub) => (StatusCode::CREATED, Json(json!(sub))).into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct ConfirmPayPalVaultSubscriptionBody {
    setup_token_id: String,
    plan_id: Uuid,
    terms_version: String,
    privacy_version: String,
}

async fn confirm_paypal_vault_subscription(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Json(body): Json<ConfirmPayPalVaultSubscriptionBody>,
) -> Response {
    if body.plan_id.is_nil() || body.setup_token_id.trim().is_empty() {
        return err_json(BffError::Validation(
            "setup_token_id and plan_id are required".into(),
        ));
    }
    let plan = match state.app.plans.get_plan(body.plan_id).await {
        Ok(plan) => plan,
        Err(error) => return err_json(error),
    };
    if plan.price_amount.trim().is_empty() || plan.trial_days <= 0 {
        return err_json(BffError::Validation(
            "PayPal Vault setup is only valid for plans with a trial".into(),
        ));
    }
    if let Err(error) = state
        .app
        .payments
        .create_paypal_payment_token(uid, body.setup_token_id, true)
        .await
    {
        return err_json(error);
    }
    match state
        .app
        .plans
        .create_subscription(uid, body.plan_id, body.terms_version, body.privacy_version)
        .await
    {
        Ok(subscription) => (
            StatusCode::CREATED,
            Json(json!({
                "subscription": subscription,
                "status": "trialing",
                "payment_method_validated": true
            })),
        )
            .into_response(),
        Err(error) => err_json(error),
    }
}

async fn cancel_subscription(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
) -> Response {
    match state.app.plans.cancel_subscription(uid, true).await {
        Ok(sub) => (StatusCode::OK, Json(json!(sub))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn get_wallet(State(state): State<HttpState>, Extension(uid): Extension<Uuid>) -> Response {
    match state.app.wallet.get_wallet(uid).await {
        Ok(wallet) => (StatusCode::OK, Json(json!(wallet))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn list_payment_methods(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
) -> Response {
    match state.app.payments.list_payment_methods(uid).await {
        Ok(methods) => {
            let count = methods.len();
            (
                StatusCode::OK,
                Json(json!({ "data": methods, "count": count })),
            )
                .into_response()
        }
        Err(e) => err_json(e),
    }
}

async fn set_default_payment_method(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    match state.app.payments.set_default_payment_method(uid, id).await {
        Ok(_) => (StatusCode::OK, Json(json!({ "success": true }))).into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct CreateTopUpBody {
    amount: String,
    currency: Option<String>,
    return_url: Option<String>,
    cancel_url: Option<String>,
}

async fn create_top_up(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Json(body): Json<CreateTopUpBody>,
) -> Response {
    if let Err(e) = validation::validate_amount(&body.amount) {
        return err_json(e);
    }
    if let Some(ref url) = body.return_url {
        if let Err(e) = validation::validate_string_length(url, 2048, "return_url") {
            return err_json(e);
        }
    }
    if let Some(ref url) = body.cancel_url {
        if let Err(e) = validation::validate_string_length(url, 2048, "cancel_url") {
            return err_json(e);
        }
    }
    let idempotency_key = Uuid::now_v7().to_string();
    match state
        .app
        .payments
        .create_top_up(
            uid,
            body.amount,
            body.currency.unwrap_or_else(|| "USD".into()),
            idempotency_key,
            body.return_url.unwrap_or_default(),
            body.cancel_url.unwrap_or_default(),
        )
        .await
    {
        Ok(resp) => (StatusCode::OK, Json(json!(resp))).into_response(),
        Err(e) => err_json(e),
    }
}

#[derive(Deserialize)]
struct ListQuery {
    limit: Option<i32>,
}

#[derive(Deserialize)]
struct ListModelsQuery {
    limit: Option<u32>,
    offset: Option<u32>,
    after_cursor: Option<String>,
}

async fn list_invoices(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Query(params): Query<ListQuery>,
) -> Response {
    let limit = params.limit.unwrap_or(100).clamp(1, 500);
    match state.app.invoices.list_invoices(uid, limit).await {
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

async fn get_invoice(State(state): State<HttpState>, Path(id): Path<Uuid>) -> Response {
    match state.app.invoices.get_invoice(id).await {
        Ok(invoice) => (StatusCode::OK, Json(json!(invoice))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn list_models(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
    Query(params): Query<ListModelsQuery>,
) -> Response {
    let subscription = match state.app.plans.get_subscription(uid).await {
        Ok(subscription) => subscription,
        Err(error) => return err_json(error),
    };
    let plan_id = match Uuid::parse_str(&subscription.plan_id) {
        Ok(plan_id) => plan_id,
        Err(error) => {
            tracing::error!(%uid, %error, "active subscription returned invalid plan id");
            return err_json(BffError::Internal(
                "active subscription has invalid plan".into(),
            ));
        }
    };
    match state
        .app
        .catalog
        .list_models(plan_id, params.limit, params.offset, params.after_cursor)
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
    Extension(uid): Extension<Uuid>,
    Path(id): Path<Uuid>,
) -> Response {
    let subscription = match state.app.plans.get_subscription(uid).await {
        Ok(subscription) => subscription,
        Err(error) => return err_json(error),
    };
    let plan_id = match Uuid::parse_str(&subscription.plan_id) {
        Ok(plan_id) => plan_id,
        Err(error) => {
            tracing::error!(%uid, %error, "active subscription returned invalid plan id");
            return err_json(BffError::Internal(
                "active subscription has invalid plan".into(),
            ));
        }
    };
    match state.app.catalog.get_model(plan_id, id).await {
        Ok(model) => (StatusCode::OK, Json(json!(model))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn current_usage(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
) -> Response {
    match state.app.usage.current_month_usage(uid).await {
        Ok(usage) => (StatusCode::OK, Json(json!(usage))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn top_models(State(state): State<HttpState>, Extension(uid): Extension<Uuid>) -> Response {
    match state.app.usage.top_models(uid).await {
        Ok(models) => (StatusCode::OK, Json(json!(models))).into_response(),
        Err(e) => err_json(e),
    }
}

async fn latency_stats(
    State(state): State<HttpState>,
    Extension(uid): Extension<Uuid>,
) -> Response {
    match state.app.usage.latency_stats(uid).await {
        Ok(stats) => (StatusCode::OK, Json(json!(stats))).into_response(),
        Err(e) => err_json(e),
    }
}
