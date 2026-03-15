use axum::{extract::Request, middleware::Next, response::Response};
use axum::http::HeaderValue;

/// Middleware that adds a unique X-Request-Id header to every request/response.
/// If the client sends one, it is preserved. Otherwise a new UUID is generated.
pub async fn request_id(mut request: Request, next: Next) -> Response {
    let id = match request.headers().get("x-request-id") {
        Some(existing) => existing.clone(),
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
