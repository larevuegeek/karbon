use axum::response::{Html, IntoResponse};
use axum::{routing::get, Router};

pub mod controller;

use karbon::http::{App, AppState};

const API_PREFIX: &str = "/api/v1";

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    App::new()
        .router(build_router())
        .serve()
        .await
}

fn build_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(controller::health::check))
        .route("/openapi.json", get(openapi_spec))
        .route("/docs", get(swagger_ui))
        // karbon:routes — server-rendered / admin controllers are mounted here
        .nest(API_PREFIX, api_routes())
}

fn api_routes() -> Router<AppState> {
    Router::new()
    // karbon:api-routes — API controllers are mounted here
}

/// OpenAPI 3.0 document, aggregated from the generated controllers.
async fn openapi_spec() -> impl IntoResponse {
    #[allow(unused_mut)]
    let mut routes: Vec<(String, String)> = Vec::new();
    // API controllers (served under API_PREFIX):
    // karbon:openapi-api
    // Root controllers (served at the root, e.g. admin):
    // karbon:openapi-root
    let refs: Vec<(&str, &str)> = routes.iter().map(|(m, p)| (m.as_str(), p.as_str())).collect();
    axum::Json(karbon::openapi::spec("{{PROJECT_NAME_TITLE}} API", "0.1.0", &refs))
}

/// Swagger UI for the API (reads /openapi.json).
async fn swagger_ui() -> impl IntoResponse {
    Html(karbon::openapi::swagger_ui_html("/openapi.json"))
}
