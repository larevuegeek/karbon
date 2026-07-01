use axum::http::HeaderValue;
use axum::{extract::Request, middleware::Next, response::Response};

/// Middleware that adds a unique X-Request-Id header to every request/response.
/// If the client sends one, it is preserved. Otherwise a new UUID is generated.
pub async fn request_id(mut request: Request, next: Next) -> Response {
    // Only accept a client-supplied id if it's short and made of safe id characters,
    // otherwise generate our own. Prevents log injection / oversized correlation ids.
    let client_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .filter(|s| {
            (1..=128).contains(&s.len())
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
        })
        .map(|s| s.to_string());

    let id = match client_id.and_then(|s| HeaderValue::from_str(&s).ok()) {
        Some(existing) => existing,
        None => {
            let id = uuid::Uuid::new_v4().to_string();
            // UUID hyphenated format is always valid ASCII, safe to unwrap
            let val = HeaderValue::from_str(&id).expect("UUID is always valid ASCII");
            request.headers_mut().insert("x-request-id", val.clone());
            val
        }
    };

    let mut response = next.run(request).await;
    response.headers_mut().insert("x-request-id", id);
    response
}
