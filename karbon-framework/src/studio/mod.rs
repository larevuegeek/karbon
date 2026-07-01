mod builder;
mod collector;
mod handlers;
pub mod middleware;

pub use builder::build;

/// The Studio auth token, injected into request extensions so the debug toolbar
/// can build an authenticated `/_studio?token=…` link.
#[derive(Clone)]
pub struct StudioToken(pub String);

/// Karbon framework version (used by the toolbar + Studio info panel).
pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Features compiled into this build (shown in the toolbar + Studio).
pub(crate) fn enabled_features() -> Vec<&'static str> {
    let mut f = Vec::new();
    if cfg!(feature = "mysql") {
        f.push("mysql");
    }
    if cfg!(feature = "postgres") {
        f.push("postgres");
    }
    if cfg!(feature = "sqlite") {
        f.push("sqlite");
    }
    if cfg!(feature = "templates") {
        f.push("templates");
    }
    if cfg!(feature = "minijinja") {
        f.push("minijinja");
    }
    if cfg!(feature = "native-templates") {
        f.push("native-templates");
    }
    if cfg!(feature = "scraping") {
        f.push("scraping");
    }
    if cfg!(feature = "redis") {
        f.push("redis");
    }
    if cfg!(feature = "studio") {
        f.push("studio");
    }
    if cfg!(feature = "cgi") {
        f.push("cgi");
    }
    f
}
pub use collector::{
    EventRecord, JobRecord, JobStatus, MailRecord, MailStatus, RequestRecord, StudioCollector,
    StudioMessage, StudioStats,
};
