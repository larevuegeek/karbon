[package]
name = "app"
version.workspace = true
edition.workspace = true

[dependencies]
framework = { package = "karbon-framework", version = "0.1" }
axum.workspace = true
tokio.workspace = true
tower-http.workspace = true
serde.workspace = true
serde_json.workspace = true
sqlx.workspace = true
chrono.workspace = true
anyhow.workspace = true
tracing.workspace = true
