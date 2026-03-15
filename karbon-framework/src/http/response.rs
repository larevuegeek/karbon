use axum::http::StatusCode;
use axum::response::IntoResponse;
use serde::Serialize;

/// Helper for common JSON responses
pub struct JsonResponse;

impl JsonResponse {
    /// 200 OK with data
    pub fn ok<T: Serialize>(data: T) -> impl IntoResponse {
        (StatusCode::OK, axum::Json(data))
    }

    /// 201 Created with data
    pub fn created<T: Serialize>(data: T) -> impl IntoResponse {
        (StatusCode::CREATED, axum::Json(data))
    }

    /// 204 No Content (for deletions)
    pub fn no_content() -> impl IntoResponse {
        StatusCode::NO_CONTENT
    }

    /// Simple message response
    pub fn message(status: StatusCode, msg: &str) -> impl IntoResponse {
        (
            status,
            axum::Json(serde_json::json!({ "message": msg })),
        )
    }

    /// Success message shorthand
    pub fn success(msg: &str) -> impl IntoResponse {
        Self::message(StatusCode::OK, msg)
    }
}
