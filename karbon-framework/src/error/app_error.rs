use std::sync::OnceLock;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;

/// Unified application error type.
///
/// Two kinds of 500: [`Internal`](Self::Internal) may carry technical detail and is masked
/// outside development; [`Failure`](Self::Failure) carries a message written for the user and
/// is always shown — build it with [`AppError::internal`], which logs the technical cause.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
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

    /// HTTP 429, message shown.
    #[error("Too many requests: {0}")]
    TooManyRequests(String),

    /// HTTP 503 (a dependency is down or not configured), message shown.
    #[error("Service unavailable: {0}")]
    ServiceUnavailable(String),

    /// HTTP 500 whose message may hold technical detail: masked outside development.
    #[error("Internal error: {0}")]
    Internal(String),

    /// HTTP 500 whose message is written for the user: always shown.
    #[error("Internal error: {0}")]
    Failure(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("JWT error: {0}")]
    Jwt(#[from] jsonwebtoken::errors::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type AppResult<T> = Result<T, AppError>;

/// Shape of the JSON body of an error response, chosen once per application.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ErrorBody {
    /// `{"error": "not_found", "message": "Not found: …"}`. The default.
    #[default]
    Typed,
    /// `{"error": "<message>", "status": 404}`: the message alone, for clients that display
    /// `error` as is.
    Flat,
}

static ERROR_BODY: OnceLock<ErrorBody> = OnceLock::new();

/// Chooses the error body for the whole application, at startup. First call wins.
pub fn set_error_body(body: ErrorBody) {
    let _ = ERROR_BODY.set(body);
}

/// The error body in use ([`ErrorBody::Typed`] unless [`set_error_body`] chose another).
pub fn error_body() -> ErrorBody {
    ERROR_BODY.get().copied().unwrap_or_default()
}

const MASKED_MESSAGE: &str = "Une erreur interne est survenue";
const INVALID_TOKEN_MESSAGE: &str = "Token invalide ou expiré";

#[derive(Serialize)]
struct TypedBody {
    error: String,
    message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<serde_json::Value>,
}

#[derive(Serialize)]
struct FlatBody {
    error: String,
    status: u16,
}

impl AppError {
    /// A 500 shown to the user as `user_message`, the technical cause going to the logs only.
    ///
    /// ```ignore
    /// repo.save(&order).await.map_err(|e| AppError::internal(e, "Impossible d'enregistrer la commande"))?;
    /// ```
    pub fn internal<E: std::fmt::Debug>(err: E, user_message: impl Into<String>) -> Self {
        let user_message = user_message.into();
        tracing::error!(error = ?err, context = %user_message, "internal error");

        Self::Failure(user_message)
    }

    /// The text a client may read, as error responses show it: the bare message, technical
    /// detail masked outside development, never why a token failed. Use it whenever an error
    /// ends up in a payload other than its own response (a list of per-item failures…):
    /// `Display` is for logs and keeps the raw detail.
    pub fn public_message(&self) -> String {
        self.client_message(ErrorBody::Flat, is_dev_environment())
    }

    fn status_code(&self) -> StatusCode {
        match self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) | Self::Validation(_) => StatusCode::BAD_REQUEST,
            Self::Unauthorized(_) | Self::Jwt(_) => StatusCode::UNAUTHORIZED,
            Self::Forbidden(_) => StatusCode::FORBIDDEN,
            Self::Conflict(_) => StatusCode::CONFLICT,
            Self::TooManyRequests(_) => StatusCode::TOO_MANY_REQUESTS,
            Self::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            Self::Internal(_) | Self::Failure(_) | Self::Database(_) | Self::Json(_) => {
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
            Self::TooManyRequests(_) => "too_many_requests",
            Self::ServiceUnavailable(_) => "service_unavailable",
            Self::Internal(_) | Self::Failure(_) | Self::Database(_) | Self::Json(_) => {
                "internal_error"
            }
        }
    }

    /// Carries technical detail that must not reach a client outside development.
    fn is_masked(&self) -> bool {
        matches!(self, Self::Internal(_) | Self::Database(_) | Self::Json(_))
    }

    /// The message alone, without the `Not found: ` prefix of `Display`.
    fn bare_message(&self) -> String {
        match self {
            Self::NotFound(m)
            | Self::BadRequest(m)
            | Self::Unauthorized(m)
            | Self::Forbidden(m)
            | Self::Conflict(m)
            | Self::Validation(m)
            | Self::TooManyRequests(m)
            | Self::ServiceUnavailable(m)
            | Self::Internal(m)
            | Self::Failure(m) => m.clone(),
            Self::Database(e) => e.to_string(),
            Self::Jwt(e) => e.to_string(),
            Self::Json(e) => e.to_string(),
        }
    }

    /// What the client reads: masked detail outside development, never why a token failed.
    fn client_message(&self, body: ErrorBody, is_dev: bool) -> String {
        if matches!(self, Self::Jwt(_)) {
            return INVALID_TOKEN_MESSAGE.to_string();
        }

        if self.is_masked() && !is_dev {
            return MASKED_MESSAGE.to_string();
        }

        match body {
            ErrorBody::Typed => self.to_string(),
            ErrorBody::Flat => self.bare_message(),
        }
    }

    fn render(&self, body: ErrorBody, is_dev: bool) -> Response {
        let status = self.status_code();
        let message = self.client_message(body, is_dev);

        match body {
            ErrorBody::Typed => {
                let body = TypedBody {
                    error: self.error_type().to_string(),
                    message,
                    details: None,
                };
                (status, axum::Json(body)).into_response()
            }
            ErrorBody::Flat => {
                let body = FlatBody {
                    error: message,
                    status: status.as_u16(),
                };
                (status, axum::Json(body)).into_response()
            }
        }
    }
}

/// `#[derive(Validate)]` errors: only the field messages are returned, nested structures
/// (`#[validate(nested)]`) included.
impl From<validator::ValidationErrors> for AppError {
    fn from(errors: validator::ValidationErrors) -> Self {
        let mut messages = Vec::new();

        collect_validation_messages(&errors, &mut messages);

        Self::Validation(messages.join(" ; "))
    }
}

fn collect_validation_messages(errors: &validator::ValidationErrors, out: &mut Vec<String>) {
    use validator::ValidationErrorsKind;

    let mut fields: Vec<_> = errors.errors().iter().collect();
    fields.sort_by(|a, b| a.0.cmp(b.0));

    for (field, kind) in fields {
        match kind {
            ValidationErrorsKind::Field(errs) => {
                for e in errs {
                    match &e.message {
                        Some(m) => out.push(m.to_string()),
                        None => out.push(format!("Valeur invalide pour {field}")),
                    }
                }
            }
            ValidationErrorsKind::Struct(nested) => collect_validation_messages(nested, out),
            ValidationErrorsKind::List(items) => {
                for nested in items.values() {
                    collect_validation_messages(nested, out);
                }
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        // Raw server errors are logged here; a `Failure` was logged by `AppError::internal`.
        if self.is_masked() {
            tracing::error!(status = %self.status_code(), error = %self, "Server error");
        }

        self.render(error_body(), is_dev_environment())
    }
}

/// Secure by default: only expose full internal detail when the environment is EXPLICITLY
/// development or test. Any other value — production, a custom env, or unset — masks server
/// errors, so a misconfigured `APP_ENV` fails safe instead of leaking raw sqlx/driver messages
/// to clients.
fn is_dev_environment() -> bool {
    crate::http::dev_mode::active()
        || matches!(
            crate::config::Environment::from_name(
                std::env::var("APP_ENV").unwrap_or_default().trim()
            ),
            crate::config::Environment::Development | crate::config::Environment::Test
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn rendered(error: AppError, body: ErrorBody, is_dev: bool) -> (u16, serde_json::Value) {
        let response = error.render(body, is_dev);
        let status = response.status().as_u16();
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();

        (status, serde_json::from_slice(&bytes).unwrap())
    }

    fn expired_token() -> AppError {
        AppError::from(jsonwebtoken::errors::Error::from(
            jsonwebtoken::errors::ErrorKind::ExpiredSignature,
        ))
    }

    /// The default body is unchanged: `error` is the type, `message` the prefixed text.
    #[tokio::test]
    async fn typed_body_is_the_default_and_unchanged() {
        assert_eq!(error_body(), ErrorBody::Typed);

        let (status, body) = rendered(
            AppError::NotFound("Contenu #3 introuvable".into()),
            ErrorBody::Typed,
            false,
        )
        .await;

        assert_eq!(status, 404);
        assert_eq!(
            body,
            serde_json::json!({"error": "not_found", "message": "Not found: Contenu #3 introuvable"})
        );
    }

    #[tokio::test]
    async fn flat_body_carries_the_bare_message_and_the_status() {
        let (status, body) = rendered(
            AppError::Conflict("Domaine déjà enregistré".into()),
            ErrorBody::Flat,
            false,
        )
        .await;

        assert_eq!(status, 409);
        assert_eq!(
            body,
            serde_json::json!({"error": "Domaine déjà enregistré", "status": 409})
        );
    }

    #[tokio::test]
    async fn new_statuses_map_to_429_and_503_with_their_message() {
        for (error, status) in [
            (AppError::TooManyRequests("Trop de tentatives".into()), 429),
            (
                AppError::ServiceUnavailable("Registrar indisponible".into()),
                503,
            ),
        ] {
            let expected = error.bare_message();
            let (got, body) = rendered(error, ErrorBody::Flat, false).await;

            assert_eq!(got, status);
            assert_eq!(body["error"], expected);
        }
    }

    /// `Internal` may hold technical detail: masked outside development, in both bodies.
    /// `Failure` is written for the user: always shown.
    #[tokio::test]
    async fn internal_is_masked_and_failure_is_shown() {
        for body in [ErrorBody::Typed, ErrorBody::Flat] {
            let (_, masked) =
                rendered(AppError::Internal("column `x` unknown".into()), body, false).await;
            let text = masked.to_string();
            assert!(
                text.contains(MASKED_MESSAGE) && !text.contains("column"),
                "{text}"
            );

            let (status, shown) = rendered(
                AppError::internal("pool timeout", "Impossible de créer la commande"),
                body,
                false,
            )
            .await;
            let text = shown.to_string();
            assert_eq!(status, 500);
            assert!(
                text.contains("Impossible de créer la commande") && !text.contains("pool timeout"),
                "{text}"
            );
        }
    }

    #[tokio::test]
    async fn database_errors_never_reach_a_client_outside_development() {
        let (status, body) = rendered(
            AppError::from(sqlx::Error::RowNotFound),
            ErrorBody::Flat,
            false,
        )
        .await;

        assert_eq!(status, 500);
        assert_eq!(body["error"], MASKED_MESSAGE);

        let (_, dev) = rendered(
            AppError::from(sqlx::Error::RowNotFound),
            ErrorBody::Flat,
            true,
        )
        .await;
        assert_ne!(dev["error"], MASKED_MESSAGE);
    }

    /// Why a token was refused (expired, bad signature…) is not the client's business, even
    /// in development.
    #[tokio::test]
    async fn token_errors_do_not_say_why() {
        for body in [ErrorBody::Typed, ErrorBody::Flat] {
            let (status, rendered) = rendered(expired_token(), body, true).await;
            let text = rendered.to_string();

            assert_eq!(status, 401);
            assert!(
                text.contains(INVALID_TOKEN_MESSAGE) && !text.contains("Expired"),
                "{text}"
            );
        }
    }

    /// For errors carried in another payload: same text as the response, never the raw detail.
    #[test]
    fn public_message_is_what_a_client_may_read() {
        assert_eq!(
            AppError::NotFound("Article introuvable".into()).public_message(),
            "Article introuvable"
        );
        assert_eq!(expired_token().public_message(), INVALID_TOKEN_MESSAGE);

        // Outside development (tests run without APP_ENV): raw errors masked.
        if !is_dev_environment() {
            assert_eq!(
                AppError::from(sqlx::Error::RowNotFound).public_message(),
                MASKED_MESSAGE
            );
        }
    }

    #[test]
    fn validation_errors_keep_the_field_messages() {
        use validator::Validate;

        #[derive(Validate)]
        struct Form {
            #[validate(length(min = 3, message = "Nom trop court"))]
            name: String,
            #[validate(email)]
            email: String,
        }

        let errors = Form {
            name: "a".into(),
            email: "nope".into(),
        }
        .validate()
        .unwrap_err();

        let AppError::Validation(message) = AppError::from(errors) else {
            panic!("expected a validation error");
        };

        assert_eq!(message, "Valeur invalide pour email ; Nom trop court");
    }
}
