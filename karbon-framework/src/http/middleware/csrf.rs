use axum::{
    extract::Request,
    http::{HeaderValue, Method, StatusCode},
    middleware::Next,
    response::{IntoResponse, Response},
};

use crate::security::Crypto;

const CSRF_HEADER: &str = "x-csrf-token";
const CSRF_COOKIE: &str = "csrf_token";

/// CSRF protection middleware.
///
/// - On safe methods (GET, HEAD, OPTIONS): sets a CSRF cookie if not already present.
/// - On unsafe methods (POST, PUT, PATCH, DELETE): validates that the `X-CSRF-Token`
///   header matches the `csrf_token` cookie.
///
/// API endpoints using only Bearer token auth (no cookies) can skip this
/// by not sending cookies at all — the middleware only enforces when a
/// CSRF cookie is present.
pub async fn csrf_protection(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let is_safe = matches!(method, Method::GET | Method::HEAD | Method::OPTIONS);

    if is_safe {
        // Check HTTPS before consuming request
        let is_https = request.uri().scheme_str() == Some("https");
        let mut response = next.run(request).await;
        // Set CSRF cookie if not already present
        if !has_csrf_cookie(&response) {
            let token = Crypto::random_token(32);
            // HttpOnly intentionally NOT set: JS must read the token to send it in X-CSRF-Token header.
            // SameSite=Strict prevents cross-site request forgery.
            let secure = if is_https { "; Secure" } else { "" };
            let cookie = format!(
                "{}={}; Path=/; SameSite=Strict; Max-Age=86400{}",
                CSRF_COOKIE, token, secure
            );
            if let Ok(val) = HeaderValue::from_str(&cookie) {
                response.headers_mut().append("set-cookie", val);
            }
        }
        return response;
    }

    // Unsafe method — validate CSRF token
    let cookie_token = extract_csrf_cookie(&request);
    let header_token = request
        .headers()
        .get(CSRF_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // If no CSRF cookie at all, the client is likely using Bearer auth — skip CSRF check
    let Some(cookie_val) = cookie_token else {
        return next.run(request).await;
    };

    // Cookie exists, so header must match
    match header_token {
        Some(header_val) if constant_time_eq(&header_val, &cookie_val) => {
            next.run(request).await
        }
        _ => {
            (StatusCode::FORBIDDEN, "CSRF token mismatch").into_response()
        }
    }
}

fn extract_csrf_cookie(request: &Request) -> Option<String> {
    let cookies = request.headers().get("cookie")?.to_str().ok()?;
    for part in cookies.split(';') {
        let part = part.trim();
        if let Some(value) = part.strip_prefix("csrf_token=") {
            return Some(value.to_string());
        }
    }
    None
}

fn has_csrf_cookie(response: &Response) -> bool {
    response
        .headers()
        .get_all("set-cookie")
        .iter()
        .any(|v| v.to_str().map_or(false, |s| s.starts_with("csrf_token=")))
}

/// Constant-time comparison to prevent timing attacks
fn constant_time_eq(a: &str, b: &str) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.bytes()
        .zip(b.bytes())
        .fold(0u8, |acc, (x, y)| acc | (x ^ y))
        == 0
}
