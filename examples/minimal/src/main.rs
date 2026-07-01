//! Minimal Karbon backend example.
//!
//! Exercises the public API end-to-end: the `karbon::` re-exports, the
//! `Insertable` derive macro and the controller/route attribute macros.
//! This crate is part of the workspace, so it is compiled by CI and acts as a
//! smoke test that generated-project code keeps compiling.
//!
//! Run with a database configured in `.env`:
//! ```bash
//! cargo run -p karbon-example-minimal
//! ```

use axum::extract::State;
use axum::response::IntoResponse;
use axum::{Json, Router};

use karbon::error::AppResult;
use karbon::http::{App, AppState, Module};

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Post {
    id: i64,
    title: String,
    slug: String,
}

/// DTO inserted into the `posts` table — `slug` is auto-derived from `title`.
/// Constructed at runtime from request bodies; here it only exercises the
/// `Insertable` derive macro at compile time.
#[allow(dead_code)]
#[derive(Debug, serde::Deserialize, karbon::Insertable)]
#[table_name("posts")]
#[timestamps]
struct NewPost {
    title: String,
    #[slug_from("title")]
    slug: String,
}

struct PostController;

#[karbon::controller(prefix = "/posts")]
impl PostController {
    #[karbon::get("/")]
    async fn list(State(_state): State<AppState>) -> AppResult<impl IntoResponse> {
        let posts: Vec<Post> = Vec::new();
        Ok(Json(serde_json::json!({ "posts": posts })))
    }
}

/// A self-contained module bundling the blog routes — the kernel extension seam.
struct BlogModule;

impl Module for BlogModule {
    fn name(&self) -> &str {
        "blog"
    }

    fn routes(&self) -> Router<AppState> {
        PostController::router()
    }

    fn prefix(&self) -> Option<&'static str> {
        Some(PostController::prefix())
    }
}

fn base_router() -> Router<AppState> {
    Router::new().route("/health", axum::routing::get(|| async { "ok" }))
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    App::new()
        .router(base_router())
        .module(BlogModule)
        .serve()
        .await
}
