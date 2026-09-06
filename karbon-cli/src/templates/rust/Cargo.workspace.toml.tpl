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
tower-http = { version = "0.6", features = ["cors", "trace", "fs"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
# Must track the version karbon-framework compiles against: `validate_request`
# takes the `Validate` trait from *that* copy of the crate.
validator = { version = "0.21.0", features = ["derive"] }
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "mysql", "chrono"] }
chrono = { version = "0.4", features = ["serde"] }
anyhow = "1"
tracing = "0.1"
