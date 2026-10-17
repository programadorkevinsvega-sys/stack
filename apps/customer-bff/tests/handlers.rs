use axum::body::Body;
use axum::http::{HeaderMap, Method, Request, StatusCode};
use axum::middleware::{from_fn, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use base64::Engine;
use customer_bff::domain::errors::BffError;
use customer_bff::interfaces::http::{
    clear_session_cookie, client_ip, err_json, err_response, extract_csrf_token,
    extract_session_cookie, generate_csrf_token, is_public_path, read_jwt_iat_exp, set_csrf_cookie,
    set_session_cookie, should_refresh_token,
};
use serde_json::Value;
use tower::ServiceExt;

// ---------------------------------------------------------------------------
// 6.4 — Pure function tests (session / cookies / helpers)
// ---------------------------------------------------------------------------

#[test]
fn client_ip_uses_x_forwarded_for() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "203.0.113.5".parse().unwrap());
    assert_eq!(client_ip(&headers), "203.0.113.5");
}

#[test]
fn client_ip_first_ip_from_chain() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-forwarded-for",
        "203.0.113.5, 198.51.100.2, 10.0.0.1".parse().unwrap(),
    );
    assert_eq!(client_ip(&headers), "203.0.113.5");
}

#[test]
fn client_ip_unknown_when_missing() {
    let headers = HeaderMap::new();
    assert_eq!(client_ip(&headers), "unknown");
}

#[test]
fn extract_session_cookie_found() {
    let mut headers = HeaderMap::new();
    headers.insert("cookie", "session_token=abc123; other=val".parse().unwrap());
    assert_eq!(extract_session_cookie(&headers).unwrap(), "abc123");
}

#[test]
fn extract_session_cookie_missing() {
    let headers = HeaderMap::new();
    assert!(matches!(
        extract_session_cookie(&headers),
        Err(BffError::AuthenticationRequired)
    ));
}

#[test]
fn extract_session_cookie_no_session_token() {
    let mut headers = HeaderMap::new();
    headers.insert("cookie", "other=val".parse().unwrap());
    assert!(matches!(
        extract_session_cookie(&headers),
        Err(BffError::AuthenticationRequired)
    ));
}

#[test]
fn extract_csrf_token_found() {
    let mut headers = HeaderMap::new();
    headers.insert("cookie", "csrf_token=t0k3n; other=val".parse().unwrap());
    assert_eq!(extract_csrf_token(&headers).unwrap(), "t0k3n");
}

#[test]
fn extract_csrf_token_missing() {
    let headers = HeaderMap::new();
    assert!(matches!(
        extract_csrf_token(&headers),
        Err(BffError::CsrfTokenMismatch)
    ));
}

#[test]
fn generate_csrf_token_is_non_empty() {
    let t = generate_csrf_token();
    assert!(!t.is_empty());
    assert_eq!(t.len(), 32);
}

#[test]
fn is_public_path_returns_true_for_known_routes() {
    assert!(is_public_path("/health"));
    assert!(is_public_path("/metrics"));
    assert!(is_public_path("/"));
    assert!(is_public_path("/api/v1/auth/register"));
    assert!(is_public_path("/api/v1/auth/login"));
    assert!(is_public_path("/api/v1/auth/refresh"));
    assert!(is_public_path("/api/v1/auth/verify-email"));
    assert!(is_public_path("/api/v1/auth/request-password-reset"));
    assert!(is_public_path(
        "/api/v1/auth/request-password-reset/confirm"
    ));
    assert!(is_public_path("/api/v1/auth/reset-password"));
}

#[test]
fn is_public_path_returns_false_for_protected_routes() {
    assert!(!is_public_path("/api/v1/me"));
    assert!(!is_public_path("/api/v1/api-keys"));
    assert!(!is_public_path("/api/v1/invoices"));
    assert!(!is_public_path("/api/v1/wallet"));
    assert!(!is_public_path("/api/v1/auth/logout"));
}

#[test]
fn set_session_cookie_includes_attributes() {
    let mut resp = (StatusCode::OK, "ok").into_response();
    set_session_cookie("t0k3n", true, None, &mut resp);
    let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("session_token=t0k3n"));
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("Secure"));
    assert!(cookie.contains("SameSite=Lax"));
    assert!(cookie.contains("Path=/"));
    assert!(cookie.contains("Max-Age=86400"));
}

#[test]
fn set_session_cookie_no_secure_when_false() {
    let mut resp = (StatusCode::OK, "ok").into_response();
    set_session_cookie("t0k3n", false, None, &mut resp);
    let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(!cookie.contains("Secure"));
}

#[test]
fn login_cookies_are_emitted_without_replacing_each_other() {
    let mut resp = (StatusCode::OK, "ok").into_response();
    set_session_cookie("session", true, Some(".example.com"), &mut resp);
    set_csrf_cookie("csrf", true, Some(".example.com"), &mut resp);

    let cookies: Vec<_> = resp.headers().get_all("set-cookie").iter().collect();
    assert_eq!(cookies.len(), 2);
    assert!(cookies
        .iter()
        .any(|cookie| cookie.to_str().unwrap().starts_with("session_token=")));
    assert!(cookies
        .iter()
        .any(|cookie| cookie.to_str().unwrap().starts_with("csrf_token=")));
}

#[test]
fn clear_session_cookie_sets_max_age_zero() {
    let mut resp = (StatusCode::OK, "ok").into_response();
    clear_session_cookie(false, None, &mut resp);
    let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("session_token="));
    assert!(cookie.contains("Max-Age=0"));
}

#[test]
fn set_csrf_cookie_includes_attributes() {
    let mut resp = (StatusCode::OK, "ok").into_response();
    set_csrf_cookie("t0k3n", false, None, &mut resp);
    let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("csrf_token=t0k3n"));
    assert!(cookie.contains("SameSite=Lax"));
    assert!(cookie.contains("Path=/"));
}

#[test]
fn set_csrf_cookie_secure_flag() {
    let mut resp = (StatusCode::OK, "ok").into_response();
    set_csrf_cookie("t0k3n", true, None, &mut resp);
    let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("Secure"));
}

#[tokio::test]
async fn err_json_returns_canonical_format() {
    let resp = err_json(BffError::InvalidCredentials);
    let body = body_text(resp).await;
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["error"], true);
    assert_eq!(v["code"], "INVALID_CREDENTIALS");
    assert_eq!(v["message"], "invalid credentials");
}

#[tokio::test]
async fn err_json_validation_returns_422() {
    let resp = err_json(BffError::Validation("bad input".into()));
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn err_response_returns_canonical_format() {
    let resp = err_response(StatusCode::NOT_FOUND, "NOT_FOUND", "resource missing");
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let body = body_text(resp).await;
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["error"], true);
    assert_eq!(v["code"], "NOT_FOUND");
    assert_eq!(v["message"], "resource missing");
}

// ---------------------------------------------------------------------------
// JWT helpers
// ---------------------------------------------------------------------------

fn make_jwt(iat: i64, exp: i64) -> String {
    let raw_payload = serde_json::json!({ "iat": iat, "exp": exp });
    let payload = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(serde_json::to_string(&raw_payload).unwrap().as_bytes());
    format!("header.{payload}.signature")
}

#[test]
fn read_jwt_iat_exp_valid() {
    let now = chrono::Utc::now().timestamp();
    let token = make_jwt(now, now + 3600);
    let (iat, exp) = read_jwt_iat_exp(&token).unwrap();
    assert_eq!(iat, now);
    assert_eq!(exp, now + 3600);
}

#[test]
fn read_jwt_iat_exp_malformed() {
    assert!(read_jwt_iat_exp("not-a-jwt").is_none());
    assert!(read_jwt_iat_exp("a.b").is_none());
    assert!(read_jwt_iat_exp("a.b.c.d").is_none());
}

#[test]
fn should_refresh_token_fresh_does_not_refresh() {
    let now = chrono::Utc::now().timestamp();
    let token = make_jwt(now, now + 3600);
    assert!(!should_refresh_token(&token));
}

#[test]
fn should_refresh_token_elapsed_half_does_refresh() {
    let now = chrono::Utc::now().timestamp();
    let token = make_jwt(now - 1801, now + 1799);
    assert!(should_refresh_token(&token));
}

#[test]
fn should_refresh_token_expired_does_refresh() {
    let now = chrono::Utc::now().timestamp();
    let token = make_jwt(now - 7200, now - 3600);
    assert!(should_refresh_token(&token));
}

#[test]
fn should_refresh_token_malformed_returns_false() {
    assert!(!should_refresh_token("bad"));
}

// ---------------------------------------------------------------------------
// 6.3 — Middleware integration tests
// ---------------------------------------------------------------------------

#[tokio::test]
async fn request_id_middleware_adds_header() {
    let app = Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(from_fn(request_id_middleware_for_test));
    let resp = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert!(resp.headers().contains_key("x-request-id"));
}

#[tokio::test]
async fn request_id_middleware_preserves_incoming() {
    let app = Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(from_fn(request_id_middleware_for_test));
    let resp = app
        .oneshot(
            Request::builder()
                .uri("/")
                .header("x-request-id", "my-id")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.headers()
            .get("x-request-id")
            .unwrap()
            .to_str()
            .unwrap(),
        "my-id"
    );
}

#[tokio::test]
async fn security_headers_middleware_adds_headers() {
    let app = Router::new()
        .route("/", get(|| async { "ok" }))
        .layer(from_fn(security_headers_middleware_for_test));
    let resp = app
        .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(
        resp.headers()
            .get("x-content-type-options")
            .unwrap()
            .to_str()
            .unwrap(),
        "nosniff"
    );
    assert_eq!(
        resp.headers()
            .get("x-frame-options")
            .unwrap()
            .to_str()
            .unwrap(),
        "DENY"
    );
    assert!(resp.headers().contains_key("strict-transport-security"));
}

#[tokio::test]
async fn err_json_response_has_content_type_json() {
    let resp = err_json(BffError::NotFound("test".into()));
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.contains("json"));
}

// ---------------------------------------------------------------------------
// CORS integration test (basic preflight)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn cors_preflight_returns_allow_headers() {
    let app = Router::new()
        .route("/api/v1/me", get(|| async { "ok" }))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::AllowOrigin::exact(
                    "http://example.com".parse().unwrap(),
                ))
                .allow_credentials(true)
                .allow_methods([Method::GET, Method::POST, Method::PUT])
                .allow_headers([
                    axum::http::header::CONTENT_TYPE,
                    axum::http::header::HeaderName::from_static("x-csrf-token"),
                ]),
        );
    let resp = app
        .oneshot(
            Request::builder()
                .method(Method::OPTIONS)
                .uri("/api/v1/me")
                .header("origin", "http://example.com")
                .header("access-control-request-method", "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    assert!(resp.headers().contains_key("access-control-allow-origin"));
    assert!(resp.headers().contains_key("access-control-allow-methods"));
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn request_id_middleware_for_test(mut req: Request<Body>, next: Next) -> Response {
    let id = req
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| uuid::Uuid::now_v7().to_string());
    req.extensions_mut().insert(id.clone());
    let mut response = next.run(req).await;
    response
        .headers_mut()
        .insert("x-request-id", id.parse().unwrap());
    response
}

async fn security_headers_middleware_for_test(req: Request<Body>, next: Next) -> Response {
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

async fn body_text(response: Response) -> String {
    let body = response.into_body();
    let bytes = axum::body::to_bytes(body, usize::MAX).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}
