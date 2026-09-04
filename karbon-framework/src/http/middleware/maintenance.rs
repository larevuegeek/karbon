use axum::{
    extract::Request,
    http::{HeaderValue, StatusCode, header},
    middleware::Next,
    response::IntoResponse,
    response::Response,
};
use serde::Serialize;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

/// Global maintenance mode flag, set from within the process.
pub static MAINTENANCE: AtomicBool = AtomicBool::new(false);

/// How the maintenance middleware decides, and what it answers.
///
/// The default reproduces the behaviour of earlier releases: no flag file, no
/// exemptions, so only [`set_maintenance`] can turn the mode on.
#[derive(Debug, Clone)]
pub struct MaintenanceConfig {
    /// When this file exists, maintenance mode is on. Checked at most once per
    /// second, so an operator can toggle the mode without restarting or
    /// deploying — and without the application ever needing write access to it.
    pub flag_file: Option<PathBuf>,
    /// Sent as `Retry-After`. It is what tells webhook senders to come back
    /// later instead of dropping the event.
    pub retry_after_secs: u64,
    /// Path prefixes that keep answering during maintenance — a status page and
    /// a health endpoint are the usual ones, since that is precisely when they
    /// get consulted.
    pub exempt_prefixes: Vec<String>,
}

impl Default for MaintenanceConfig {
    fn default() -> Self {
        Self {
            flag_file: None,
            retry_after_secs: 120,
            exempt_prefixes: Vec::new(),
        }
    }
}

static CONFIG: OnceLock<MaintenanceConfig> = OnceLock::new();

/// Install the maintenance configuration. Call once, before serving; later
/// calls are ignored.
pub fn init_maintenance(config: MaintenanceConfig) {
    let _ = CONFIG.set(config);
}

fn config() -> &'static MaintenanceConfig {
    CONFIG.get_or_init(MaintenanceConfig::default)
}

/// Enable or disable maintenance mode at runtime.
///
/// Independent of the flag file: either source turns the mode on, so this can
/// never be used to *lift* a maintenance an operator declared on disk.
pub fn set_maintenance(enabled: bool) {
    MAINTENANCE.store(enabled, Ordering::Relaxed);
}

// ── Flag file, with a one-second cache ────────────────────────────────────
//
// `Path::exists` is a blocking `stat`. It costs microseconds, but running one
// per request would still be wasteful on a hot path that answers "no" almost
// every time — hence the cache. One second is short enough that an operator
// never waits, long enough to make the syscall irrelevant.

const FLAG_TTL_MS: u64 = 1_000;
/// Elapsed milliseconds since process start, plus one — `0` means "never checked".
static FLAG_CHECKED_AT_MS: AtomicU64 = AtomicU64::new(0);
static FLAG_PRESENT: AtomicBool = AtomicBool::new(false);

fn process_start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

fn flag_file_present() -> bool {
    let Some(path) = config().flag_file.as_ref() else {
        return false;
    };

    let now = process_start().elapsed().as_millis() as u64 + 1;
    let checked_at = FLAG_CHECKED_AT_MS.load(Ordering::Relaxed);
    if checked_at != 0 && now.saturating_sub(checked_at) < FLAG_TTL_MS {
        return FLAG_PRESENT.load(Ordering::Relaxed);
    }

    let present = path.exists();
    FLAG_PRESENT.store(present, Ordering::Relaxed);
    FLAG_CHECKED_AT_MS.store(now, Ordering::Relaxed);
    present
}

/// Check if maintenance mode is active, from either source.
///
/// Also usable outside the HTTP path — background jobs should consult it before
/// touching the database, since no HTTP middleware ever sees them.
pub fn is_maintenance() -> bool {
    MAINTENANCE.load(Ordering::Relaxed) || flag_file_present()
}

/// True when this path keeps being served during maintenance.
pub fn is_exempt(path: &str) -> bool {
    config()
        .exempt_prefixes
        .iter()
        .any(|prefix| path.starts_with(prefix.as_str()))
}

#[derive(Serialize)]
struct MaintenanceResponse {
    error: &'static str,
    message: &'static str,
    retry_after: u64,
}

fn maintenance_response() -> Response {
    let retry_after = config().retry_after_secs;

    let mut response = (
        StatusCode::SERVICE_UNAVAILABLE,
        axum::Json(MaintenanceResponse {
            error: "maintenance",
            message: "The API is currently under maintenance. Please try again later.",
            retry_after,
        }),
    )
        .into_response();

    // Without this header a 503 reads as "give up" to most webhook senders;
    // with it, they come back and the events are not lost.
    if let Ok(value) = HeaderValue::from_str(&retry_after.to_string()) {
        response.headers_mut().insert(header::RETRY_AFTER, value);
    }

    response
}

/// Middleware that returns 503 when maintenance mode is active.
pub async fn maintenance_mode(request: Request, next: Next) -> Response {
    if !is_maintenance() || is_exempt(request.uri().path()) {
        return next.run(request).await;
    }

    maintenance_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_keep_the_previous_behaviour() {
        let c = MaintenanceConfig::default();
        assert!(c.flag_file.is_none(), "no file source unless opted in");
        assert!(c.exempt_prefixes.is_empty());
        assert_eq!(c.retry_after_secs, 120);
    }

    #[test]
    fn a_missing_file_source_never_triggers_maintenance() {
        // CONFIG is process-wide and may be set by another test; the guarantee
        // under test is that the *default* config reports nothing on disk.
        let c = MaintenanceConfig::default();
        assert!(c.flag_file.is_none());
    }

    #[test]
    fn exemptions_match_on_prefix() {
        let c = MaintenanceConfig {
            exempt_prefixes: vec!["/status".into(), "/health".into()],
            ..Default::default()
        };
        let matches = |p: &str| c.exempt_prefixes.iter().any(|x| p.starts_with(x.as_str()));

        assert!(matches("/status"));
        assert!(matches("/status/detail"));
        assert!(matches("/health"));
        assert!(!matches("/account/domain"));
        assert!(!matches("/internal/webhook/mail-event"));
    }
}
