[package]
name = "app"
version.workspace = true
edition.workspace = true

[dependencies]
# `studio` = dev cockpit (/_studio) + debug toolbar. Compiled in, but mounted only in
# debug builds — `karbon build`/`serve` (release) never expose it.
karbon = { package = "karbon-framework", version = "{{KARBON_VERSION}}", features = ["studio"] }
axum.workspace = true
tokio.workspace = true
tower-http.workspace = true
serde.workspace = true
serde_json.workspace = true
sqlx.workspace = true
chrono.workspace = true
anyhow.workspace = true
tracing.workspace = true
