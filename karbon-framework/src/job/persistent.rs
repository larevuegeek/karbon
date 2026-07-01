//! Persistent (DB-backed) background jobs.
//!
//! Unlike [`super::JobQueue`] (in-process, lost on restart), jobs pushed here
//! are stored in a `_karbon_jobs` table and survive restarts. A worker polls
//! the table, claims a job (claim-by-delete), deserializes it by `KIND` and runs
//! its handler, re-queuing with an incremented attempt count on failure.
//!
//! ```ignore
//! #[derive(serde::Serialize, serde::Deserialize)]
//! struct SendEmail { to: String }
//! impl PersistentJob for SendEmail {
//!     const KIND: &'static str = "send_email";
//!     fn run(&self) -> Pin<Box<dyn Future<Output = AppResult<()>> + Send + '_>> {
//!         Box::pin(async move { /* … */ Ok(()) })
//!     }
//! }
//!
//! let queue = PersistentQueue::builder(pool).register::<SendEmail>().build();
//! queue.migrate().await?;          // create the table
//! queue.push(&SendEmail { to: "a@b.c".into() }).await?;
//! queue.start(Duration::from_secs(1)); // background worker
//! ```

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::db::{DbPool, InsertBuilder};
use crate::error::{AppError, AppResult};

const JOBS_TABLE: &str = "_karbon_jobs";

/// A job that can be serialized, persisted and run later.
pub trait PersistentJob: Serialize + DeserializeOwned + Send + Sync + 'static {
    /// Stable identifier used to route a stored payload back to its handler.
    const KIND: &'static str;

    /// Execute the job.
    fn run(&self) -> Pin<Box<dyn Future<Output = AppResult<()>> + Send + '_>>;
}

type HandlerFn =
    Arc<dyn Fn(String) -> Pin<Box<dyn Future<Output = AppResult<()>> + Send>> + Send + Sync>;

/// Builder for a [`PersistentQueue`] — register every job type before `build`.
pub struct PersistentQueueBuilder {
    pool: DbPool,
    handlers: HashMap<&'static str, HandlerFn>,
    max_attempts: u32,
}

impl PersistentQueueBuilder {
    pub fn register<J: PersistentJob>(mut self) -> Self {
        let handler: HandlerFn = Arc::new(|payload: String| {
            Box::pin(async move {
                let job: J = serde_json::from_str(&payload)
                    .map_err(|e| AppError::Internal(format!("deserialize {}: {e}", J::KIND)))?;
                job.run().await
            })
        });
        self.handlers.insert(J::KIND, handler);
        self
    }

    /// Max attempts before a job is dropped to the dead-letter log (default 3).
    pub fn max_attempts(mut self, n: u32) -> Self {
        self.max_attempts = n;
        self
    }

    pub fn build(self) -> PersistentQueue {
        PersistentQueue {
            pool: self.pool,
            handlers: Arc::new(self.handlers),
            max_attempts: self.max_attempts,
        }
    }
}

/// A DB-backed job queue. Cheap to clone.
#[derive(Clone)]
pub struct PersistentQueue {
    pool: DbPool,
    handlers: Arc<HashMap<&'static str, HandlerFn>>,
    max_attempts: u32,
}

impl PersistentQueue {
    pub fn builder(pool: DbPool) -> PersistentQueueBuilder {
        PersistentQueueBuilder {
            pool,
            handlers: HashMap::new(),
            max_attempts: 3,
        }
    }

    /// Create the `_karbon_jobs` table if it does not exist.
    pub async fn migrate(&self) -> AppResult<()> {
        #[cfg(feature = "mysql")]
        let id_col = "id BIGINT AUTO_INCREMENT PRIMARY KEY";
        #[cfg(feature = "postgres")]
        let id_col = "id BIGSERIAL PRIMARY KEY";
        #[cfg(feature = "sqlite")]
        let id_col = "id INTEGER PRIMARY KEY AUTOINCREMENT";

        let sql = format!(
            "CREATE TABLE IF NOT EXISTS {JOBS_TABLE} (\
             {id_col}, \
             kind VARCHAR(255) NOT NULL, \
             payload TEXT NOT NULL, \
             attempts INT NOT NULL DEFAULT 0, \
             created_at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP)"
        );
        sqlx::query(&sql)
            .execute(&self.pool)
            .await
            .map_err(|e| AppError::Internal(format!("create jobs table: {e}")))?;
        Ok(())
    }

    /// Enqueue a job for later execution.
    pub async fn push<J: PersistentJob>(&self, job: &J) -> AppResult<()> {
        let payload = serde_json::to_string(job)
            .map_err(|e| AppError::Internal(format!("serialize {}: {e}", J::KIND)))?;
        InsertBuilder::into(JOBS_TABLE)
            .set("kind", J::KIND.to_string())
            .set("payload", payload)
            .set("attempts", 0_i64)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Spawn a background worker that polls and runs jobs.
    pub fn start(&self, poll_interval: Duration) {
        let queue = self.clone();
        tokio::spawn(async move {
            loop {
                match queue.process_one().await {
                    Ok(true) => continue, // got a job — try the next immediately
                    Ok(false) => {}       // idle
                    Err(e) => tracing::error!(error = %e, "persistent job worker error"),
                }
                tokio::time::sleep(poll_interval).await;
            }
        });
    }

    /// Claim and run a single job. Returns whether a job was processed.
    pub async fn process_one(&self) -> AppResult<bool> {
        let row: Option<(i64, String, String, i64)> = sqlx::query_as(&format!(
            "SELECT id, kind, payload, attempts FROM {JOBS_TABLE} ORDER BY id LIMIT 1"
        ))
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| AppError::Internal(format!("fetch job: {e}")))?;

        let Some((id, kind, payload, attempts)) = row else {
            return Ok(false);
        };

        // Claim by delete: if another worker already took it, rows_affected is 0.
        let claimed = sqlx::query(&format!(
            "DELETE FROM {JOBS_TABLE} WHERE id = {}",
            placeholder1()
        ))
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| AppError::Internal(format!("claim job: {e}")))?;
        if claimed.rows_affected() == 0 {
            return Ok(false);
        }

        let Some(handler) = self.handlers.get(kind.as_str()) else {
            tracing::error!(kind = %kind, "no handler registered for job kind (dropped)");
            return Ok(true);
        };

        match handler(payload.clone()).await {
            Ok(()) => tracing::debug!(kind = %kind, "persistent job done"),
            Err(e) => {
                let next = attempts + 1;
                if (next as u32) < self.max_attempts {
                    tracing::warn!(kind = %kind, attempt = next, error = %e, "job failed, re-queuing");
                    let _ = InsertBuilder::into(JOBS_TABLE)
                        .set("kind", kind)
                        .set("payload", payload)
                        .set("attempts", next)
                        .execute(&self.pool)
                        .await;
                } else {
                    tracing::error!(kind = %kind, attempts = next, error = %e, "job failed permanently (dead-letter)");
                }
            }
        }
        Ok(true)
    }
}

fn placeholder1() -> String {
    crate::db::placeholder(1)
}
