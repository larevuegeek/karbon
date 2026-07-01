[package]
name = "app"
version.workspace = true
edition.workspace = true

[dependencies]
# Tip: add  features = ["studio"]  to enable the dev dashboard + debug toolbar.
# (`karbon new --local <path>` does this automatically against a local checkout.)
karbon = { package = "karbon-framework", version = "0.3.1" }
axum.workspace = true
tokio.workspace = true
tower-http.workspace = true
serde.workspace = true
serde_json.workspace = true
sqlx.workspace = true
chrono.workspace = true
anyhow.workspace = true
tracing.workspace = true
