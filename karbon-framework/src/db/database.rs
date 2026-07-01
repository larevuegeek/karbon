use super::{DbPool, DbPoolOptions};
use crate::config::Config;

/// Database wrapper with connection pool
#[derive(Debug, Clone)]
pub struct Database {
    pool: DbPool,
}

impl Database {
    /// Connect to the database using app config
    pub async fn connect(config: &Config) -> Result<Self, sqlx::Error> {
        let pool = DbPoolOptions::new()
            .max_connections(config.db_max_connections)
            .connect(&config.database_url())
            .await?;

        Ok(Self { pool })
    }

    /// Connect with a raw URL
    pub async fn connect_url(url: &str) -> Result<Self, sqlx::Error> {
        let pool = DbPoolOptions::new().max_connections(5).connect(url).await?;

        Ok(Self { pool })
    }

    /// Build a **lazy** pool that connects on first use instead of immediately.
    ///
    /// Used for apps that don't configure a database: the pool exists (so the
    /// `AppState` is well-formed) but never connects unless a query is run.
    pub fn connect_lazy(config: &Config) -> Result<Self, sqlx::Error> {
        let pool = DbPoolOptions::new()
            .max_connections(config.db_max_connections.max(1))
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
