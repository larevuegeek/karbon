use axum::Router;
use axum::routing::{get, post};

use super::collector::StudioCollector;
use super::handlers::{self, StudioState};

/// Build the Studio router.
///
/// Returns `(router, collector, token)`:
/// - Mount the router at `/_studio`
/// - Inject the collector into request extensions via the studio middleware
/// - The token is printed to the console for the developer
///
/// ```ignore
/// use karbon::studio;
///
/// let (studio_router, collector, token) = studio::build();
/// tracing::info!("Studio: http://localhost:{}/_studio?token={}", port, token);
///
/// let app = Router::new()
///     .nest("/_studio", studio_router)
///     .layer(middleware::from_fn(studio::middleware::studio_middleware))
///     .layer(Extension(collector));
/// ```
pub fn build(db: Option<crate::db::DbPool>) -> (Router, StudioCollector, String) {
    build_with_token(db, generate_token())
}

/// Like [`build`], with a token the operator chose (`KARBON_STUDIO_TOKEN`) instead of a
/// random one. A release build no longer logs the token, so a deployment that enables
/// Studio with `KARBON_STUDIO` alone needs a way to know it.
pub fn build_with_token(
    db: Option<crate::db::DbPool>,
    token: String,
) -> (Router, StudioCollector, String) {
    let collector = StudioCollector::new();

    let state = StudioState {
        collector: collector.clone(),
        token: token.clone(),
        db,
    };

    let router = Router::new()
        .route("/", get(handlers::dashboard))
        .route("/styles.css", get(handlers::styles))
        .route("/app.js", get(handlers::script))
        .route("/ws", get(handlers::ws_handler))
        .route("/api/data", get(handlers::api_data))
        .route("/api/stats", get(handlers::api_stats))
        .route("/api/info", get(handlers::api_info))
        .route("/api/database", get(handlers::api_database))
        .route("/api/terminal", post(handlers::api_terminal))
        .route("/api/docs", get(handlers::api_docs))
        .route("/api/clear", post(handlers::api_clear))
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            handlers::studio_guard,
        ))
        .with_state(state);

    (router, collector, token)
}

fn generate_token() -> String {
    use rand::RngExt;
    let mut rng = rand::rng();
    (0..32)
        .map(|_| {
            let idx: u8 = rng.random_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect()
}
