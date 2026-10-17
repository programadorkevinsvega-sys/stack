use admin_bff::domain::errors::BffError;
use admin_bff::interfaces::http::{
    clear_session_cookie, client_ip, err_json, err_response, extract_csrf_token,
    extract_session_cookie, generate_csrf_token, is_public_path, read_jwt_iat_exp, set_csrf_cookie,
    set_session_cookie, should_refresh_token,
};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use jsonwebtoken::{encode, EncodingKey, Header};
use serde::Serialize;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn client_ip_from_x_forwarded_for() {
    let mut headers = HeaderMap::new();
    headers.insert("x-forwarded-for", "203.0.113.42".parse().unwrap());
    assert_eq!(client_ip(&headers), "203.0.113.42");
}

#[test]
fn client_ip_from_chain() {
    let mut headers = HeaderMap::new();
    headers.insert(
        "x-forwarded-for",
        "203.0.113.42, 10.0.0.1, 192.168.1.1".parse().unwrap(),
    );
    assert_eq!(client_ip(&headers), "203.0.113.42");
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
fn extract_session_cookie_missing_header() {
    let headers = HeaderMap::new();
    assert!(extract_session_cookie(&headers).is_err());
}

#[test]
fn extract_csrf_token_found() {
    let mut headers = HeaderMap::new();
    headers.insert("cookie", "csrf_token=token123; other=val".parse().unwrap());
    assert_eq!(extract_csrf_token(&headers).unwrap(), "token123");
}

#[test]
fn extract_csrf_token_missing() {
    let mut headers = HeaderMap::new();
    headers.insert("cookie", "session_token=abc".parse().unwrap());
    assert!(extract_csrf_token(&headers).is_err());
}

#[test]
fn generate_csrf_token_length() {
    let token = generate_csrf_token();
    assert_eq!(token.len(), 32);
}

#[test]
fn is_public_path_returns_true_for_public_routes() {
    assert!(is_public_path("/"));
    assert!(is_public_path("/health"));
    assert!(is_public_path("/metrics"));
    assert!(is_public_path("/api/v1/auth/login"));
    assert!(is_public_path("/api/v1/auth/refresh"));
}

#[test]
fn is_public_path_returns_false_for_admin_routes() {
    assert!(!is_public_path("/api/v1/admin/users"));
    assert!(!is_public_path("/api/v1/me"));
}

#[test]
fn set_session_cookie_has_secure_flag() {
    let mut resp = Response::default();
    set_session_cookie("tok", true, None, &mut resp);
    let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("HttpOnly"));
    assert!(cookie.contains("Secure"));
    assert!(cookie.contains("session_token=tok"));
}

#[test]
fn set_session_cookie_without_secure() {
    let mut resp = Response::default();
    set_session_cookie("tok", false, None, &mut resp);
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
    let mut resp = Response::default();
    clear_session_cookie(true, None, &mut resp);
    let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("Max-Age=0"));
}

#[test]
fn set_csrf_cookie_has_secure_flag() {
    let mut resp = Response::default();
    set_csrf_cookie("tok", true, None, &mut resp);
    let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("csrf_token=tok"));
    assert!(cookie.contains("Secure"));
}

#[test]
fn err_json_returns_canonical_format() {
    let e = BffError::Validation("bad input".into());
    let resp = err_json(e);
    assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[test]
fn err_json_content_type() {
    let e = BffError::NotFound("item".into());
    let resp = err_json(e);
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[test]
fn err_response_returns_correct_status() {
    let resp = err_response(
        StatusCode::UNAUTHORIZED,
        "AUTH_REQUIRED",
        "authentication required",
    );
    assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
}

fn make_test_jwt(iat: u64, exp: u64) -> String {
    #[derive(Serialize)]
    struct Claims {
        iat: u64,
        exp: u64,
        sub: String,
    }
    let claims = Claims {
        iat,
        exp,
        sub: "test".into(),
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(b"test"),
    )
    .unwrap()
}

#[test]
fn read_jwt_iat_exp_valid() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let token = make_test_jwt(now, now + 3600);
    let (iat, exp) = read_jwt_iat_exp(&token).unwrap();
    assert_eq!(iat, now as i64);
    assert_eq!(exp, (now + 3600) as i64);
}

#[test]
fn read_jwt_iat_exp_malformed() {
    assert!(read_jwt_iat_exp("not.a.jwt").is_none());
}

#[test]
fn should_refresh_token_fresh() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let token = make_test_jwt(now, now + 3600);
    assert!(!should_refresh_token(&token));
}

#[test]
fn should_refresh_token_half_expired() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let token = make_test_jwt(now - 1801, now + 1799);
    assert!(should_refresh_token(&token));
}

#[test]
fn should_refresh_token_expired() {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let token = make_test_jwt(now - 7200, now - 3600);
    assert!(should_refresh_token(&token));
}

#[test]
fn should_refresh_token_malformed() {
    assert!(!should_refresh_token("bad.token"));
}

#[test]
fn err_json_returns_internal_server_error() {
    let e = BffError::Internal("db error".into());
    let resp = err_json(e);
    assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn err_json_returns_forbidden_for_csrf() {
    let e = BffError::CsrfTokenMismatch;
    let resp = err_json(e);
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[test]
fn err_json_returns_too_many_requests() {
    let e = BffError::RateLimited;
    let resp = err_json(e);
    assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[test]
fn set_session_cookie_with_domain() {
    let mut resp = Response::default();
    set_session_cookie("tok", true, Some("example.com"), &mut resp);
    let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("Domain=example.com"));
}

#[test]
fn clear_session_cookie_with_domain() {
    let mut resp = Response::default();
    clear_session_cookie(true, Some("example.com"), &mut resp);
    let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("Domain=example.com"));
    assert!(cookie.contains("Max-Age=0"));
}

#[test]
fn set_csrf_cookie_with_domain() {
    let mut resp = Response::default();
    set_csrf_cookie("tok", true, Some("example.com"), &mut resp);
    let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("Domain=example.com"));
}

#[test]
fn set_session_cookie_max_age() {
    let mut resp = Response::default();
    set_session_cookie("tok", true, None, &mut resp);
    let cookie = resp.headers().get("set-cookie").unwrap().to_str().unwrap();
    assert!(cookie.contains("Max-Age=86400"));
}
