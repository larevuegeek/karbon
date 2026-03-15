use axum::{extract::Request, middleware::Next, response::Response};
use std::time::Instant;

/// Request logging middleware — logs method, path, status, and duration
pub async fn request_logger(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let uri = request.uri().path().to_string();
    let start = Instant::now();

    let response = next.run(request).await;

    let duration = start.elapsed();
    let status = response.status();

    tracing::info!(
        method = %method,
        path = %uri,
        status = %status.as_u16(),
        duration_ms = %duration.as_millis(),
        "Request"
    );

    response
}
