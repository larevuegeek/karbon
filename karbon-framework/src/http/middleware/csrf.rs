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
        // Check HTTPS before consuming request — honor a TLS-terminating reverse proxy
        // that forwards over plain HTTP with X-Forwarded-Proto: https.
        let is_https = request.uri().scheme_str() == Some("https")
            || request
                .headers()
                .get("x-forwarded-proto")
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.eq_ignore_ascii_case("https"));
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

    // Unsafe method. Pure Bearer-token API clients (no cookies) are exempt — cross-site
    // attackers cannot set an Authorization header from a browser.
    if has_bearer_auth(&request) {
        return next.run(request).await;
    }

    // Defense-in-depth: reject cross-site Origin/Referer outright. Browsers always send
    // Origin on unsafe cross-origin requests, so this blocks classic CSRF even before the
    // first CSRF cookie has been issued (e.g. a bare login POST).
    if let Some(false) = origin_is_same_site(&request) {
        return (StatusCode::FORBIDDEN, "Cross-site request rejected").into_response();
    }

    // Double-submit: if a CSRF cookie is present, the X-CSRF-Token header must match it.
    let cookie_token = extract_csrf_cookie(&request);
    let header_token = request
        .headers()
        .get(CSRF_HEADER)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    match cookie_token {
        Some(cookie_val) => match header_token {
            Some(header_val) if constant_time_eq(&header_val, &cookie_val) => {
                next.run(request).await
            }
            _ => (StatusCode::FORBIDDEN, "CSRF token mismatch").into_response(),
        },
        // No cookie yet and same-origin (or no Origin header): allow. The Origin check
        // above already rejected cross-site attempts.
        None => next.run(request).await,
    }
}

fn has_bearer_auth(request: &Request) -> bool {
    request
        .headers()
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.to_ascii_lowercase().starts_with("bearer "))
}

/// Returns `Some(true)` if the Origin/Referer host matches the request Host,
/// `Some(false)` if it is cross-site, and `None` if no Origin/Referer is present.
fn origin_is_same_site(request: &Request) -> Option<bool> {
    let host = request
        .headers()
        .get("host")
        .and_then(|v| v.to_str().ok())?;
    let source = request
        .headers()
        .get("origin")
        .or_else(|| request.headers().get("referer"))
        .and_then(|v| v.to_str().ok())?;
    // Extract host[:port] from the Origin/Referer URL.
    let after_scheme = source.split("://").nth(1).unwrap_or(source);
    let source_host = after_scheme
        .split(['/', '?', '#'])
        .next()
        .unwrap_or(after_scheme);
    Some(source_host.eq_ignore_ascii_case(host))
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
        .any(|v| v.to_str().is_ok_and(|s| s.starts_with("csrf_token=")))
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
