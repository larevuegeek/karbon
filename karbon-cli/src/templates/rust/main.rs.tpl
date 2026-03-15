use axum::{routing::get, Router};
use tower_http::services::ServeDir;

pub mod controller;

use framework::http::{App, AppState};

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
        .nest(API_PREFIX, api_routes())
}

fn api_routes() -> Router<AppState> {
    Router::new()
}
