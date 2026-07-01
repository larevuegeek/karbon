//! Message bus (à la Symfony Messenger).
//!
//! A unified command/message bus with **fallible handlers**, two transports
//! (synchronous and background), retry/backoff and dead-letter logging. It
//! generalizes both the [`crate::event::EventBus`] (fan-out, fire-and-forget)
//! and the [`crate::job::JobQueue`] (background work with retry).
//!
//! ```ignore
//! use karbon::message::{Message, MessageBus};
//!
//! struct SendWelcome { email: String }
//! impl Message for SendWelcome {}
//!
//! let bus = MessageBus::new(4); // 4 background workers
//! bus.handle::<SendWelcome, _, _>(|msg| async move {
//!     send_email(&msg.email).await?;
//!     Ok(())
//! }).await;
//!
//! bus.dispatch(SendWelcome { email: "a@b.c".into() }).await?; // run now, await result
//! bus.dispatch_async(SendWelcome { email: "a@b.c".into() }).await; // run in background
//! ```

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::{RwLock, mpsc};

/// Marker trait for messages. Implement it on your message/command structs.
pub trait Message: Send + Sync + 'static {}

type AnyMsg = Arc<dyn Any + Send + Sync>;
type BoxFuture = Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send>>;
type Handler = Arc<dyn Fn(AnyMsg) -> BoxFuture + Send + Sync>;

/// Retry strategy applied to background (`dispatch_async`) handling.
#[derive(Clone, Copy, Debug)]
pub struct RetryPolicy {
    /// Number of retries after the first attempt (0 = no retry).
    pub max_retries: u32,
    /// Delay between attempts.
    pub backoff: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 0,
            backoff: Duration::from_secs(1),
        }
    }
}

struct Queued {
    type_id: TypeId,
    msg: AnyMsg,
    name: &'static str,
}

/// A unified async message bus with synchronous and background transports.
#[derive(Clone)]
pub struct MessageBus {
    handlers: Arc<RwLock<HashMap<TypeId, Vec<Handler>>>>,
    sender: mpsc::Sender<Queued>,
}

impl MessageBus {
    /// Create a bus with `workers` background workers and the default retry policy.
    pub fn new(workers: usize) -> Self {
        Self::with_retry(workers, RetryPolicy::default())
    }

    /// Create a bus with a custom [`RetryPolicy`] for background handling.
    pub fn with_retry(workers: usize, retry: RetryPolicy) -> Self {
        let handlers: Arc<RwLock<HashMap<TypeId, Vec<Handler>>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let (sender, mut receiver) = mpsc::channel::<Queued>(256);

        let worker_handlers = handlers.clone();
        tokio::spawn(async move {
            let semaphore = Arc::new(tokio::sync::Semaphore::new(workers.max(1)));
            while let Some(queued) = receiver.recv().await {
                let sem = semaphore.clone();
                let handlers = worker_handlers.clone();
                tokio::spawn(async move {
                    let Ok(_permit) = sem.acquire().await else {
                        return;
                    };
                    let list = handlers
                        .read()
                        .await
                        .get(&queued.type_id)
                        .cloned()
                        .unwrap_or_default();
                    for handler in list {
                        run_with_retry(&handler, &queued, retry).await;
                    }
                });
            }
        });

        Self { handlers, sender }
    }

    /// Register a fallible async handler for message type `M`. Multiple handlers
    /// may be registered for the same message type.
    pub async fn handle<M, F, Fut>(&self, handler: F)
    where
        M: Message,
        F: Fn(Arc<M>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<()>> + Send + 'static,
    {
        let handler = Arc::new(handler);
        let boxed: Handler = Arc::new(move |msg: AnyMsg| {
            let handler = handler.clone();
            Box::pin(async move {
                match msg.downcast::<M>() {
                    Ok(m) => handler(m).await,
                    Err(_) => Ok(()),
                }
            }) as BoxFuture
        });

        self.handlers
            .write()
            .await
            .entry(TypeId::of::<M>())
            .or_default()
            .push(boxed);
    }

    /// Dispatch a message **synchronously**: run every handler in order and
    /// await the result. Returns the first handler error, if any.
    pub async fn dispatch<M: Message>(&self, msg: M) -> anyhow::Result<()> {
        let msg: AnyMsg = Arc::new(msg);
        let handlers = self.handlers.read().await.get(&TypeId::of::<M>()).cloned();
        if let Some(list) = handlers {
            for handler in list {
                handler(msg.clone()).await?;
            }
        }
        Ok(())
    }

    /// Dispatch a message to the **background** transport: it is queued and
    /// handled by a worker with retry/backoff. Fire-and-forget.
    pub async fn dispatch_async<M: Message>(&self, msg: M) {
        let queued = Queued {
            type_id: TypeId::of::<M>(),
            msg: Arc::new(msg),
            name: std::any::type_name::<M>(),
        };
        if self.sender.send(queued).await.is_err() {
            tracing::error!("MessageBus is closed; message dropped");
        }
    }
}

async fn run_with_retry(handler: &Handler, queued: &Queued, retry: RetryPolicy) {
    for attempt in 0..=retry.max_retries {
        match handler(queued.msg.clone()).await {
            Ok(()) => return,
            Err(e) => {
                if attempt < retry.max_retries {
                    tracing::warn!(
                        message = queued.name,
                        attempt = attempt + 1,
                        max = retry.max_retries,
                        error = %e,
                        "Message handler failed, retrying..."
                    );
                    tokio::time::sleep(retry.backoff).await;
                } else {
                    tracing::error!(
                        message = queued.name,
                        error = %e,
                        "Message handler failed after {} attempt(s) (dead-letter)",
                        retry.max_retries + 1
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct Ping;
    impl Message for Ping {}

    #[tokio::test]
    async fn sync_dispatch_runs_all_handlers() {
        let bus = MessageBus::new(2);
        let counter = Arc::new(AtomicU32::new(0));

        for _ in 0..2 {
            let c = counter.clone();
            bus.handle::<Ping, _, _>(move |_msg| {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .await;
        }

        bus.dispatch(Ping).await.unwrap();
        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn sync_dispatch_propagates_error() {
        let bus = MessageBus::new(1);
        bus.handle::<Ping, _, _>(|_msg| async { Err(anyhow::anyhow!("boom")) })
            .await;
        let res = bus.dispatch(Ping).await;
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn async_dispatch_retries_then_succeeds() {
        let bus = MessageBus::with_retry(
            1,
            RetryPolicy {
                max_retries: 3,
                backoff: Duration::from_millis(1),
            },
        );
        let attempts = Arc::new(AtomicU32::new(0));
        let a = attempts.clone();
        bus.handle::<Ping, _, _>(move |_msg| {
            let a = a.clone();
            async move {
                // Fail the first two attempts, succeed on the third.
                let n = a.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err(anyhow::anyhow!("transient"))
                } else {
                    Ok(())
                }
            }
        })
        .await;

        bus.dispatch_async(Ping).await;

        // Give the worker time to retry.
        for _ in 0..100 {
            if attempts.load(Ordering::SeqCst) >= 3 {
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
        assert_eq!(attempts.load(Ordering::SeqCst), 3);
    }
}
