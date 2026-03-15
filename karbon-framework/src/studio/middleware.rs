use axum::extract::{ConnectInfo, Request};
use axum::middleware::Next;
use axum::response::Response;
use std::net::SocketAddr;
use std::time::Instant;

use super::collector::StudioCollector;

/// Middleware that records every request/response into the StudioCollector.
/// Skips studio's own routes to avoid noise.
pub async fn studio_middleware(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path().to_string();

    // Don't record studio's own requests
    if path.starts_with("/_studio") {
        return next.run(request).await;
    }

    let method = request.method().to_string();
    let request_id = request
        .headers()
        .get("x-request-id")
        .and_then(|v| v.to_str().ok())
        .map(String::from);

    let request_headers: Vec<(String, String)> = request
        .headers()
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("<binary>").to_string()))
        .collect();

    let collector = request.extensions().get::<StudioCollector>().cloned();

    let start = Instant::now();
    let response = next.run(request).await;
    let duration = start.elapsed();

    if let Some(collector) = collector {
        let status = response.status().as_u16();
        let response_headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap_or("<binary>").to_string()))
            .collect();

        let remote = addr.to_string();

        tokio::spawn(async move {
            collector
                .record_request(
                    method,
                    path,
                    status,
                    duration,
                    request_headers,
                    response_headers,
                    request_id,
                    Some(remote),
                )
                .await;
        });
    }

    response
}
