use super::pool_settings::env_max_connections;
use super::{DbPool, DbPoolOptions, PoolSettings};
use crate::config::Config;

/// Pool ceiling `connect_url` has always used. Kept as the fallback so that
/// deployments which never set `DB_MAX_CONNECTIONS` are unaffected.
const URL_FALLBACK_MAX_CONNECTIONS: u32 = 5;

/// URL schemes the compiled driver accepts in [`Database::connect_lazy_url_with`].
#[cfg(feature = "mysql")]
const URL_SCHEMES: &[&str] = &["mysql://", "mariadb://"];
#[cfg(feature = "postgres")]
const URL_SCHEMES: &[&str] = &["postgres://", "postgresql://"];
#[cfg(feature = "sqlite")]
const URL_SCHEMES: &[&str] = &["sqlite:"];

/// Database wrapper with connection pool
#[derive(Debug, Clone)]
pub struct Database {
    pool: DbPool,
}

impl Database {
    /// Connect to the database using app config
    pub async fn connect(config: &Config) -> Result<Self, sqlx::Error> {
        Self::connect_with(config, &PoolSettings::from_env(config.db_max_connections)).await
    }

    /// Connect to the database using app config and explicit pool settings.
    pub async fn connect_with(
        config: &Config,
        settings: &PoolSettings,
    ) -> Result<Self, sqlx::Error> {
        let pool = settings
            .apply(DbPoolOptions::new())
            .connect(&config.database_url())
            .await?;

        Ok(Self { pool })
    }

    /// Connect with a raw URL
    ///
    /// Honours `DB_MAX_CONNECTIONS`, falling back to `5` — the value this method
    /// has always hard-coded — when the variable is unset.
    pub async fn connect_url(url: &str) -> Result<Self, sqlx::Error> {
        let max = env_max_connections(URL_FALLBACK_MAX_CONNECTIONS);
        Self::connect_url_with(url, &PoolSettings::from_env(max)).await
    }

    /// Connect with a raw URL and explicit pool settings.
    pub async fn connect_url_with(url: &str, settings: &PoolSettings) -> Result<Self, sqlx::Error> {
        let pool = settings.apply(DbPoolOptions::new()).connect(url).await?;

        Ok(Self { pool })
    }

    /// Build a **lazy** pool that connects on first use instead of immediately.
    ///
    /// Used for apps that don't configure a database: the pool exists (so the
    /// `AppState` is well-formed) but never connects unless a query is run.
    pub fn connect_lazy(config: &Config) -> Result<Self, sqlx::Error> {
        Self::connect_lazy_with(
            config,
            &PoolSettings::from_env(config.db_max_connections.max(1)),
        )
    }

    /// Lazy pool with explicit settings.
    pub fn connect_lazy_with(
        config: &Config,
        settings: &PoolSettings,
    ) -> Result<Self, sqlx::Error> {
        let pool = settings
            .apply(DbPoolOptions::new())
            .connect_lazy(&config.database_url())?;
        Ok(Self { pool })
    }

    /// Lazy pool from a raw URL: nothing connects until the first query, but a malformed URL
    /// fails here — including one whose scheme is not the compiled driver's.
    ///
    /// The scheme check is what makes that true for SQLite: sqlx takes any string without a
    /// `sqlite:` prefix as a file path, so a typo would pass and only fail at the first query
    /// (or silently open whatever file it happens to name). It also catches a URL meant for
    /// another driver, e.g. a `postgres://` URL in a MySQL build.
    pub fn connect_lazy_url_with(url: &str, settings: &PoolSettings) -> Result<Self, sqlx::Error> {
        let has_scheme = URL_SCHEMES.iter().any(|scheme| {
            url.get(..scheme.len())
                .is_some_and(|prefix| prefix.eq_ignore_ascii_case(scheme))
        });
        if !has_scheme {
            // The value is not quoted: a database URL usually carries a password.
            return Err(sqlx::Error::Configuration(
                format!(
                    "database URL must start with {} for this build",
                    URL_SCHEMES.join(" or ")
                )
                .into(),
            ));
        }
        let pool = settings.apply(DbPoolOptions::new()).connect_lazy(url)?;

        Ok(Self { pool })
    }

    /// Lazy pool from the URL held by the environment variable `var`, for applications that
    /// talk to several databases.
    ///
    /// - `Ok(None)`: the variable is unset or blank — an optional database the app runs
    ///   without (the caller decides what is mandatory);
    /// - `Err`: the URL is malformed — a typo stops the startup instead of silently disabling
    ///   whatever needed that database.
    ///
    /// ```ignore
    /// let main = Database::lazy_from_env("DATABASE_URL", &PoolSettings::from_env(10))?
    ///     .context("DATABASE_URL must be set")?;
    /// let dns = Database::lazy_from_env("DATABASE_URL_DNS", &PoolSettings::from_env(5))?;
    /// ```
    pub fn lazy_from_env(var: &str, settings: &PoolSettings) -> Result<Option<Self>, sqlx::Error> {
        let Some(url) = std::env::var(var).ok().filter(|url| !url.trim().is_empty()) else {
            tracing::info!("{var} is not set: running without this database");
            return Ok(None);
        };

        Self::connect_lazy_url_with(&url, settings).map(Some)
    }

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &DbPool {
        &self.pool
    }

    /// The connection pool itself, for state that stores pools rather than `Database`.
    pub fn into_pool(self) -> DbPool {
        self.pool
    }

    /// Run pending migrations from a directory path
    pub async fn migrate(&self, path: &str) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate::Migrator::new(std::path::Path::new(path))
            .await?
            .run(&self.pool)
            .await
    }

    /// Start a new transaction
    pub async fn begin(&self) -> Result<sqlx::Transaction<'_, super::Db>, sqlx::Error> {
        self.pool.begin().await
    }

    /// Health check — ping the database
    pub async fn ping(&self) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> PoolSettings {
        PoolSettings::from_env(2)
    }

    #[test]
    fn an_unset_or_blank_variable_is_an_absent_database() {
        assert!(
            Database::lazy_from_env("KARBON_TEST_DB_UNSET_7F3A", &settings())
                .unwrap()
                .is_none()
        );

        // SAFETY: variable name unique to this test; no other thread reads it.
        unsafe { std::env::set_var("KARBON_TEST_DB_BLANK_7F3A", "  ") };
        assert!(
            Database::lazy_from_env("KARBON_TEST_DB_BLANK_7F3A", &settings())
                .unwrap()
                .is_none()
        );
    }

    #[test]
    fn a_malformed_url_is_an_error_not_an_absent_database() {
        // SAFETY: variable name unique to this test; no other thread reads it.
        unsafe { std::env::set_var("KARBON_TEST_DB_TYPO_7F3A", "not a database url") };

        assert!(Database::lazy_from_env("KARBON_TEST_DB_TYPO_7F3A", &settings()).is_err());
    }

    /// A URL the compiled driver accepts, pointing nowhere that answers.
    #[cfg(feature = "mysql")]
    const VALID_URL: &str = "mysql://user:secret@127.0.0.1:1/none";
    #[cfg(feature = "postgres")]
    const VALID_URL: &str = "postgres://user:secret@127.0.0.1:1/none";
    #[cfg(feature = "sqlite")]
    const VALID_URL: &str = "sqlite::memory:";

    /// A URL for one of the other drivers.
    #[cfg(feature = "mysql")]
    const OTHER_DRIVER_URL: &str = "postgres://user:hunter2@db/app";
    #[cfg(not(feature = "mysql"))]
    const OTHER_DRIVER_URL: &str = "mysql://user:hunter2@db/app";

    /// Lazy: a valid URL to a server that does not exist builds a pool without connecting.
    #[tokio::test]
    async fn a_valid_url_builds_a_pool_without_connecting() {
        // SAFETY: variable name unique to this test; no other thread reads it.
        unsafe { std::env::set_var("KARBON_TEST_DB_OK_7F3A", VALID_URL) };

        let database = Database::lazy_from_env("KARBON_TEST_DB_OK_7F3A", &settings())
            .unwrap()
            .expect("configured database");

        assert_eq!(database.into_pool().size(), 0);
    }

    /// Under SQLite any string is a valid file path to sqlx, so before the scheme check
    /// this typo was accepted and only failed at the first query.
    #[test]
    fn a_string_without_the_driver_scheme_is_refused() {
        for value in ["not a database url", "/var/data/app.db", "app.db"] {
            assert!(
                Database::connect_lazy_url_with(value, &settings()).is_err(),
                "{value:?} was accepted"
            );
        }
    }

    #[test]
    fn a_url_for_another_driver_is_refused_without_leaking_it() {
        let err = Database::connect_lazy_url_with(OTHER_DRIVER_URL, &settings())
            .expect_err("a URL for another driver must be refused");
        let message = err.to_string();
        assert!(!message.contains("hunter2"), "password leaked: {message}");
        assert!(message.contains(URL_SCHEMES[0]), "{message}");
    }
}
