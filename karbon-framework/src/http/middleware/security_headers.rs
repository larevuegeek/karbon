use axum::{extract::Request, http::HeaderValue, middleware::Next, response::Response};

/// Adds a baseline set of security response headers to every response.
///
/// - `X-Content-Type-Options: nosniff` — stop MIME sniffing (blocks sniffed-XSS on uploads).
/// - `X-Frame-Options: DENY` — clickjacking protection (framing).
/// - `Referrer-Policy: strict-origin-when-cross-origin` — limit referrer leakage.
/// - `Strict-Transport-Security` — only over HTTPS, to avoid pinning HSTS on plain-HTTP dev.
///
/// A `Content-Security-Policy` is intentionally NOT forced here because a sane CSP is
/// app-specific (inline scripts, frontend origins). Set one per-app when ready.
pub async fn security_headers(request: Request, next: Next) -> Response {
    let is_https = request.uri().scheme_str() == Some("https")
        || request
            .headers()
            .get("x-forwarded-proto")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.eq_ignore_ascii_case("https"));

    let mut response = next.run(request).await;
    let headers = response.headers_mut();

    headers
        .entry("x-content-type-options")
        .or_insert_with(|| HeaderValue::from_static("nosniff"));
    headers
        .entry("x-frame-options")
        .or_insert_with(|| HeaderValue::from_static("DENY"));
    headers
        .entry("referrer-policy")
        .or_insert_with(|| HeaderValue::from_static("strict-origin-when-cross-origin"));

    if is_https {
        headers
            .entry("strict-transport-security")
            .or_insert_with(|| HeaderValue::from_static("max-age=31536000; includeSubDomains"));
    }

    response
}
