[workspace]
resolver = "2"
members = ["app"]

[workspace.package]
version = "0.1.0"
edition = "2024"
license = "MIT"

[workspace.dependencies]
axum = { version = "0.8", features = ["multipart", "macros"] }
tokio = { version = "1", features = ["full"] }
tower-http = { version = "0.7", features = ["cors", "trace", "fs"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
# `validator` and `sqlx` must track the versions karbon-framework compiles against:
# their types cross the framework boundary (`Validate`, `DbPool`), so two copies in
# the dependency graph are two incompatible sets of types.
validator = { version = "0.21.0", features = ["derive"] }
sqlx = { version = "0.9.0", features = ["runtime-tokio", "tls-rustls", "mysql", "chrono"] }
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
tracing = "0.1"
