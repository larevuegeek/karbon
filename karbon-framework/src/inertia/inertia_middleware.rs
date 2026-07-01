use axum::extract::Request;
use axum::http::StatusCode;
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use super::inertia_response::{INERTIA_HEADER, INERTIA_VERSION_HEADER, InertiaConfig};

/// Middleware that handles Inertia.js protocol requirements:
///
/// 1. **Version check** — If the client sends `X-Inertia-Version` and it differs
///    from the server version, responds with 409 Conflict to trigger a full reload.
///
/// 2. **Redirect conversion** — Converts 302 redirects to 303 for PUT/PATCH/DELETE
///    requests (Inertia protocol requirement).
///
/// ```ignore
/// use karbon::inertia::{inertia_middleware, InertiaConfig};
///
/// let config = InertiaConfig::new(include_str!("../templates/app.html"))
///     .version("1.0.0");
///
/// let app = Router::new()
///     .route("/", get(home))
///     .layer(Extension(config))
///     .layer(middleware::from_fn(inertia_middleware));
/// ```
pub async fn inertia_middleware(request: Request, next: Next) -> Response {
    let is_inertia = request.headers().contains_key(INERTIA_HEADER);

    // Version conflict check
    if is_inertia
        && let Some(config) = request.extensions().get::<InertiaConfig>()
        && let Some(client_version) = request.headers().get(INERTIA_VERSION_HEADER)
        && let Ok(client_ver) = client_version.to_str()
        && !config.version.is_empty()
        && client_ver != config.version
    {
        // Version mismatch → force full page reload
        return (StatusCode::CONFLICT, [(INERTIA_HEADER, "true")], "").into_response();
    }

    let method = request.method().clone();
    let mut response = next.run(request).await;

    // Convert 302 to 303 for PUT/PATCH/DELETE (Inertia protocol)
    if is_inertia
        && response.status() == StatusCode::FOUND
        && matches!(
            method,
            axum::http::Method::PUT | axum::http::Method::PATCH | axum::http::Method::DELETE
        )
    {
        *response.status_mut() = StatusCode::SEE_OTHER;
    }

    response
}
