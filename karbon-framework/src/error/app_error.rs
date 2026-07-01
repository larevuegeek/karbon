use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Unified application error type
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Bad request: {0}")]
    BadRequest(String),

    #[error("Unauthorized: {0}")]
    Unauthorized(String),

    #[error("Forbidden: {0}")]
    Forbidden(String),

    #[error("Conflict: {0}")]
    Conflict(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("JWT error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type AppResult<T> = Result<T, AppError>;

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<serde_json::Value>,
}

impl AppError {
    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) | Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) | Self::Jwt(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::Internal(_) | Self::Database(_) | Self::Json(_) => {
                StatusCode::INTERNAL_SERVER_ERROR
            }
        }
    }

    fn error_type(&self) -> &str {
        match self {
            Self::NotFound(_) => "not_found",
            Self::BadRequest(_) => "bad_request",
            Self::Unauthorized(_) | Self::Jwt(_) => "unauthorized",
            Self::Forbidden(_) => "forbidden",
            Self::Conflict(_) => "conflict",
            Self::Validation(_) => "validation_error",
            Self::Internal(_) | Self::Database(_) | Self::Json(_) => "internal_error",
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = self.status_code();
        let error_type = self.error_type().to_string();

        // Log server errors
        if status.is_server_error() {
            tracing::error!(%status, error = %self, "Server error");
        }

        // Secure by default: only expose full internal detail when the environment is
        // EXPLICITLY development or test. Any other value — production, a custom env, or
        // unset — masks server errors, so a misconfigured `APP_ENV` fails safe instead of
        // leaking raw sqlx/driver messages to clients.
        let is_dev = crate::http::dev_mode::active()
            || matches!(
                crate::config::Environment::from_name(
                    std::env::var("APP_ENV").unwrap_or_default().trim()
                ),
                crate::config::Environment::Development | crate::config::Environment::Test
            );
        let message = if status.is_server_error() && !is_dev {
            "Une erreur interne est survenue".to_string()
        } else {
            self.to_string()
        };

        let body = ErrorResponse {
            error: error_type,
            message,
            details: None,
        };

        (status, axum::Json(body)).into_response()
    }
}
