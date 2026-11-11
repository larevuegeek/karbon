//! Background jobs.
//!
//! [`PersistentQueue`] — database-backed, survives a restart — is the job queue to use.
//!
//! The in-memory [`JobQueue`] and its [`Job`] trait are deprecated since 0.5.0:
//! [`crate::message::MessageBus::dispatch_async`] does the same work (background workers,
//! retry with backoff, dead-letter log) with plain message structs and closures. The
//! retry policy moves from each job (`max_retries`, `retry_delay`) to the bus
//! ([`crate::message::RetryPolicy`]).

// The module still defines and tests what it deprecates.
#![allow(deprecated)]

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

mod persistent;
pub use persistent::{PersistentJob, PersistentQueue, PersistentQueueBuilder};

/// Trait for background jobs.
///
/// ```ignore
/// struct SendEmailJob { pub to: String, pub subject: String }
///
/// impl Job for SendEmailJob {
///     fn name(&self) -> &str { "send_email" }
///
///     fn execute(&self) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send + '_>> {
///         Box::pin(async {
///             // send email logic
///             Ok(())
///         })
///     }
/// }
/// ```
#[deprecated(
    since = "0.5.0",
    note = "use a `karbon::message::Message` handled by `MessageBus::dispatch_async`, or `PersistentJob` for jobs that must survive a restart"
)]
pub trait Job: Send + Sync + 'static {
    fn name(&self) -> &str;
    fn execute(&self) -> Pin<Box<dyn Future<Output = Result<(), anyhow::Error>> + Send + '_>>;

    /// Number of retries on failure (default: 0)
    fn max_retries(&self) -> u32 {
        0
    }

    /// Delay between retries (default: 5s)
    fn retry_delay(&self) -> Duration {
        Duration::from_secs(5)
    }
}

/// In-process background job queue using tokio tasks.
///
/// ```ignore
/// let queue = JobQueue::new(4); // 4 workers
/// queue.push(SendEmailJob { to: "foo@bar.com".into(), subject: "Hello".into() }).await;
/// // job runs in background, no need to await
///
/// queue.shutdown().await; // wait for all jobs to finish
/// ```
#[deprecated(
    since = "0.5.0",
    note = "use `karbon::message::MessageBus::dispatch_async` (retry via `RetryPolicy`), or `PersistentQueue` for jobs that must survive a restart"
)]
pub struct JobQueue {
    sender: mpsc::Sender<Arc<dyn Job>>,
    shutdown: Option<tokio::task::JoinHandle<()>>,
}

impl JobQueue {
    /// Create a new job queue with the given number of concurrent workers
    pub fn new(workers: usize) -> Self {
        let (sender, receiver) = mpsc::channel::<Arc<dyn Job>>(256);
        let receiver = Arc::new(tokio::sync::Mutex::new(receiver));

        let handle = tokio::spawn(async move {
            let semaphore = Arc::new(tokio::sync::Semaphore::new(workers));
            let recv = receiver;

            loop {
                let job = {
                    let mut rx = recv.lock().await;
                    rx.recv().await
                };

                let Some(job) = job else { break };
                let sem = semaphore.clone();

                tokio::spawn(async move {
                    let Ok(_permit) = sem.acquire().await else {
                        return;
                    };
                    let name = job.name().to_string();
                    let max_retries = job.max_retries();

                    for attempt in 0..=max_retries {
                        match job.execute().await {
                            Ok(()) => {
                                tracing::debug!(job = %name, "Job completed");
                                return;
                            }
                            Err(e) => {
                                if attempt < max_retries {
                                    tracing::warn!(
                                        job = %name,
                                        attempt = attempt + 1,
                                        max = max_retries,
                                        error = %e,
                                        "Job failed, retrying..."
                                    );
                                    tokio::time::sleep(job.retry_delay()).await;
                                } else {
                                    tracing::error!(
                                        job = %name,
                                        error = %e,
                                        "Job failed after {} attempts",
                                        max_retries + 1
                                    );
                                }
                            }
                        }
                    }
                });
            }
        });

        Self { sender, shutdown: Some(handle) }
    }

    /// Push a job to the queue for background execution
    pub async fn push<J: Job>(&self, job: J) {
        if self.sender.send(Arc::new(job)).await.is_err() {
            tracing::error!("Job queue is closed");
        }
    }

    /// Shut down the queue and wait for all pending jobs
    pub async fn shutdown(mut self) {
        drop(self.sender.clone());
        if let Some(handle) = self.shutdown.take() {
            let _ = handle.await;
        }
    }
}
