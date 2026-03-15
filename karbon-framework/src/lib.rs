pub mod config;
pub mod db;
pub mod error;
pub mod http;
pub mod security;
pub mod validation;
pub mod cache;
pub mod logger;
pub mod mail;
pub mod storage;
pub mod util;
pub mod event;
pub mod job;
pub mod i18n;
pub mod testing;
pub mod channel;
pub mod feature;
pub mod inertia;
pub mod livewire;
pub mod hmr;
#[cfg(feature = "studio")]
pub mod studio;

// Re-exports for convenience
pub use http::{App, AppState};
pub use db::Database;
pub use error::{AppError, AppResult};
pub use channel::ChannelRegistry;
pub use feature::FeatureFlags;

// Re-export macros
pub use karbon_macros::{controller, get, post, put, delete, patch, require_role};
pub use karbon_macros::{Insertable, Updatable};
