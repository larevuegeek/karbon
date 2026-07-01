use axum::{
    extract::Request, http::StatusCode, middleware::Next, response::IntoResponse,
    response::Response,
};
use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global maintenance mode flag
pub static MAINTENANCE: AtomicBool = AtomicBool::new(false);

/// Enable or disable maintenance mode at runtime
pub fn set_maintenance(enabled: bool) {
    MAINTENANCE.store(enabled, Ordering::Relaxed);
}

/// Check if maintenance mode is active
pub fn is_maintenance() -> bool {
    MAINTENANCE.load(Ordering::Relaxed)
}

#[derive(Serialize)]
struct MaintenanceResponse {
    error: &'static str,
    message: &'static str,
}

/// Middleware that returns 503 when maintenance mode is active
pub async fn maintenance_mode(request: Request, next: Next) -> Response {
    if is_maintenance() {
        let body = MaintenanceResponse {
            error: "maintenance",
            message: "The API is currently under maintenance. Please try again later.",
        };
        return (StatusCode::SERVICE_UNAVAILABLE, axum::Json(body)).into_response();
    }

    next.run(request).await
}
