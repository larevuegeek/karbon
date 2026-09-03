use std::time::Duration;

use super::DbPoolOptions;

/// Connection-pool tuning, read from the environment.
///
/// Every default reproduces the effective behaviour of earlier Karbon releases
/// (which relied on sqlx's own defaults), so an application that sets none of
/// these variables observes no change after upgrading.
///
/// | Variable | Default | Purpose |
/// |---|---|---|
/// | `DB_MIN_CONNECTIONS` | `0` | connections kept open even when idle |
/// | `DB_ACQUIRE_TIMEOUT_SECS` | `30` | how long a caller waits for a free connection |
/// | `DB_MAX_LIFETIME_SECS` | `1800` | forced recycling; `0` disables it |
/// | `DB_IDLE_TIMEOUT_SECS` | `600` | idle reaping; `0` disables it |
///
/// `max_connections` is not read here: callers already resolve it (from
/// [`Config`](crate::config::Config) or from `DB_MAX_CONNECTIONS`) and pass it in,
/// so an explicitly built config is never silently overridden by the environment.
///
/// # Clustered databases
///
/// `max_lifetime` is what makes a pool survive a node failover. After the router
/// moves the writer, the pool still holds sockets to the previous node; recycling
/// is what spreads it back onto the live one instead of discovering each dead
/// connection one request at a time. Keep it **below the server's own
/// `wait_timeout`**, otherwise the server closes first and the application pays
/// the discovery anyway.
///
/// `acquire_timeout` deserves an explicit value for the same reason: during a
/// failover, callers queue on the pool, and the 30 s default is far longer than
/// most front-ends are willing to wait.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PoolSettings {
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout: Duration,
    /// `None` lets a connection live until it breaks.
    pub max_lifetime: Option<Duration>,
    /// `None` never reaps idle connections.
    pub idle_timeout: Option<Duration>,
}

impl PoolSettings {
    pub const DEFAULT_ACQUIRE_TIMEOUT_SECS: u64 = 30;
    pub const DEFAULT_MAX_LIFETIME_SECS: u64 = 30 * 60;
    pub const DEFAULT_IDLE_TIMEOUT_SECS: u64 = 10 * 60;

    /// Read the timing knobs from the environment, around a caller-supplied ceiling.
    pub fn from_env(max_connections: u32) -> Self {
        Self {
            max_connections: max_connections.max(1),
            min_connections: env_parse("DB_MIN_CONNECTIONS", 0),
            acquire_timeout: Duration::from_secs(
                env_parse(
                    "DB_ACQUIRE_TIMEOUT_SECS",
                    Self::DEFAULT_ACQUIRE_TIMEOUT_SECS,
                )
                .max(1),
            ),
            max_lifetime: optional_secs("DB_MAX_LIFETIME_SECS", Self::DEFAULT_MAX_LIFETIME_SECS),
            idle_timeout: optional_secs("DB_IDLE_TIMEOUT_SECS", Self::DEFAULT_IDLE_TIMEOUT_SECS),
        }
    }

    /// Apply these settings to a pool builder.
    pub fn apply(&self, options: DbPoolOptions) -> DbPoolOptions {
        options
            .max_connections(self.max_connections)
            .min_connections(self.min_connections.min(self.max_connections))
            .acquire_timeout(self.acquire_timeout)
            .max_lifetime(self.max_lifetime)
            .idle_timeout(self.idle_timeout)
    }
}

/// Reads `DB_MAX_CONNECTIONS`, falling back to the caller's historical value.
pub(super) fn env_max_connections(fallback: u32) -> u32 {
    env_parse("DB_MAX_CONNECTIONS", fallback)
}

/// A duration in seconds where `0` means "disabled" rather than "immediately".
fn optional_secs(key: &str, default: u64) -> Option<Duration> {
    match env_parse(key, default) {
        0 => None,
        secs => Some(Duration::from_secs(secs)),
    }
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(default)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole point of the defaults: an app that sets no variable must get
    /// exactly what sqlx handed it before these settings existed.
    #[test]
    fn defaults_match_sqlx_behaviour() {
        let s = PoolSettings {
            max_connections: 7,
            min_connections: 0,
            acquire_timeout: Duration::from_secs(PoolSettings::DEFAULT_ACQUIRE_TIMEOUT_SECS),
            max_lifetime: Some(Duration::from_secs(PoolSettings::DEFAULT_MAX_LIFETIME_SECS)),
            idle_timeout: Some(Duration::from_secs(PoolSettings::DEFAULT_IDLE_TIMEOUT_SECS)),
        };

        let reference = DbPoolOptions::new().max_connections(7);
        let applied = s.apply(DbPoolOptions::new());

        assert_eq!(
            applied.get_max_connections(),
            reference.get_max_connections()
        );
        assert_eq!(
            applied.get_min_connections(),
            reference.get_min_connections()
        );
        assert_eq!(
            applied.get_acquire_timeout(),
            reference.get_acquire_timeout()
        );
        assert_eq!(applied.get_max_lifetime(), reference.get_max_lifetime());
        assert_eq!(applied.get_idle_timeout(), reference.get_idle_timeout());
    }

    #[test]
    fn zero_disables_recycling() {
        assert_eq!(optional_secs("KARBON_TEST_UNSET_LIFETIME", 0), None);
        assert_eq!(
            optional_secs("KARBON_TEST_UNSET_LIFETIME", 42),
            Some(Duration::from_secs(42))
        );
    }

    #[test]
    fn min_connections_never_exceeds_max() {
        let s = PoolSettings {
            max_connections: 3,
            min_connections: 99,
            acquire_timeout: Duration::from_secs(30),
            max_lifetime: None,
            idle_timeout: None,
        };
        assert_eq!(s.apply(DbPoolOptions::new()).get_min_connections(), 3);
    }

    #[test]
    fn max_connections_has_a_floor_of_one() {
        assert_eq!(PoolSettings::from_env(0).max_connections, 1);
    }
}
