use super::pool_settings::env_max_connections;
use super::{DbPool, DbPoolOptions, PoolSettings};
use crate::config::Config;

/// Pool ceiling `connect_url` has always used. Kept as the fallback so that
/// deployments which never set `DB_MAX_CONNECTIONS` are unaffected.
const URL_FALLBACK_MAX_CONNECTIONS: u32 = 5;

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

    /// Get a reference to the connection pool
    pub fn pool(&self) -> &DbPool {
        &self.pool
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
