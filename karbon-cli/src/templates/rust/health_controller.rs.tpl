use axum::response::IntoResponse;
use axum::Json;

pub async fn check() -> impl IntoResponse {
    Json(serde_json::json!({
        "status": "ok",
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
