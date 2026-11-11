use axum::{extract::Request, middleware::Next, response::Response};
use std::time::Instant;

/// Profiling middleware — active in debug builds only.
///
/// Logs each request with:
/// - HTTP method + path
/// - Status code
/// - Execution time (warning above 100 ms)
/// - Response size
///
/// ```ignore
/// use axum::middleware;
/// use karbon::logger::profiler_middleware;
///
/// Router::new()
///     .layer(middleware::from_fn(profiler_middleware))
/// ```
///
/// Output:
/// ```text
/// [DEBUG] GET /admin/categories → 200 OK (2ms) [156 bytes]
/// [SLOW]  GET /admin/users → 200 OK (152ms) [4.2 KB]
/// [DEBUG] POST /auth/login → 401 WARN (12ms) [64 bytes]
/// ```
pub async fn profiler_middleware(request: Request, next: Next) -> Response {
    // In a release build the middleware is a no-op
    if !cfg!(debug_assertions) {
        return next.run(request).await;
    }

    let method = request.method().clone();
    let uri = request.uri().path().to_string();
    let start = Instant::now();

    let response = next.run(request).await;

    let duration = start.elapsed();
    let status = response.status().as_u16();
    let duration_ms = duration.as_millis();

    // Response size
    let size = response
        .headers()
        .get(axum::http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<u64>().ok());

    let size_str = match size {
        Some(bytes) if bytes >= 1024 => format!("{:.1} KB", bytes as f64 / 1024.0),
        Some(bytes) => format!("{} bytes", bytes),
        None => "-".to_string(),
    };

    let status_tag = if status >= 500 {
        "ERR"
    } else if status >= 400 {
        "WARN"
    } else {
        "OK"
    };

    if duration_ms > 100 {
        tracing::warn!("[SLOW] {} {} → {} {} ({}ms) [{}]", method, uri, status, status_tag, duration_ms, size_str);
    } else {
        tracing::debug!("[DEBUG] {} {} → {} {} ({}ms) [{}]", method, uri, status, status_tag, duration_ms, size_str);
    }

    response
}
