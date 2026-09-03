//! # Karbon
//!
//! Karbon is a batteries-included, full-stack web framework for Rust, built on
//! top of [Axum]. It bundles everything needed to ship a real application:
//! routing via attribute macros, a lightweight ORM, authentication (JWT +
//! Argon2), validation, file storage, mail, background jobs, an event bus,
//! realtime channels, i18n and more — behind a single unified `karbon` CLI for
//! `dev`, `build`, `serve`, `generate`, `migrate` and `deploy`.
//!
//! ## Quick start
//!
//! ```ignore
//! use karbon::http::{App, AppState};
//! use axum::{Router, routing::get};
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     App::new()
//!         .router(Router::new().route("/health", get(|| async { "ok" })))
//!         .serve()
//!         .await
//! }
//! ```
//!
//! ## Database drivers
//!
//! Karbon supports MySQL/MariaDB (default) and PostgreSQL through mutually
//! exclusive feature flags:
//!
//! - `mysql` (default) — `karbon-framework = "..."`
//! - `postgres` — `default-features = false, features = ["postgres"]`
//!
//! Optional features: `studio` (dev dashboard) and `templates` (server-side
//! rendering).
//!
//! ## Environments
//!
//! [`config::load_env`] loads the `.env` family in cascade
//! (`.env` → `.env.{env}` → `.env.local` → `.env.{env}.local`) based on
//! `APP_ENV`, mirroring Symfony's behavior. See [`config::Environment`].
//!
//! [Axum]: https://docs.rs/axum

pub mod cache;
#[cfg(feature = "cgi")]
pub mod cgi;
pub mod channel;
pub mod config;
pub mod db;
pub mod error;
pub mod event;
pub mod feature;
pub mod form;
pub mod hmr;
pub mod http;
pub mod i18n;
pub mod inertia;
pub mod job;
pub mod livewire;
pub mod logger;
pub mod mail;
pub mod message;
pub mod openapi;
#[cfg(feature = "scraping")]
pub mod scraping;
pub mod security;
pub mod storage;
#[cfg(feature = "studio")]
pub mod studio;
#[cfg(any(
    feature = "templates",
    feature = "minijinja",
    feature = "native-templates"
))]
pub mod template;
pub mod testing;
pub mod util;
pub mod validation;

// Re-exports for convenience
pub use channel::ChannelRegistry;
pub use db::{Database, PoolSettings};
pub use error::{AppError, AppResult};
pub use feature::FeatureFlags;
pub use form::{Field, Form};
pub use http::{App, AppState, Bundle, Module};
pub use message::{Message, MessageBus, RetryPolicy};

// Re-exported dependencies used by the derive/attribute macros, so generated
// code only ever needs the `karbon` crate in scope (not these transitively).
#[doc(hidden)]
pub use slug;
#[doc(hidden)]
pub use tracing;

// Re-export macros
pub use karbon_macros::{Insertable, Updatable};
pub use karbon_macros::{controller, delete, get, patch, post, put, require_role};
