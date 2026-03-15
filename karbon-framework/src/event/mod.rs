use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Trait for events. Implement this on your event structs.
///
/// ```ignore
/// struct UserCreated { pub user_id: i64, pub email: String }
/// impl Event for UserCreated {}
/// ```
pub trait Event: Send + Sync + 'static {}

type BoxHandler = Arc<dyn Fn(&dyn Any) -> Pin<Box<dyn Future<Output = ()> + Send>> + Send + Sync>;

/// Simple async event bus (pub/sub).
///
/// ```ignore
/// let bus = EventBus::new();
///
/// bus.on::<UserCreated>(|event| async move {
///     println!("User created: {}", event.email);
/// }).await;
///
/// bus.emit(UserCreated { user_id: 1, email: "foo@bar.com".into() }).await;
/// ```
#[derive(Clone)]
pub struct EventBus {
    handlers: Arc<RwLock<HashMap<TypeId, Vec<BoxHandler>>>>,
}

impl EventBus {
    pub fn new() -> Self {
        Self {
            handlers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a handler for an event type
    pub async fn on<E, F, Fut>(&self, handler: F)
    where
        E: Event,
        F: Fn(Arc<E>) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let handler = Arc::new(handler);
        let handler: BoxHandler = Arc::new(move |event: &dyn Any| -> Pin<Box<dyn Future<Output = ()> + Send>> {
            if let Some(e) = event.downcast_ref::<Arc<E>>() {
                let e = e.clone();
                let h = handler.clone();
                Box::pin(async move { h(e).await })
            } else {
                Box::pin(async {})
            }
        });

        let mut handlers = self.handlers.write().await;
        handlers.entry(TypeId::of::<E>()).or_default().push(handler);
    }

    /// Emit an event, calling all registered handlers concurrently
    pub async fn emit<E: Event>(&self, event: E) {
        let event = Arc::new(event);
        let handlers = self.handlers.read().await;
        if let Some(list) = handlers.get(&TypeId::of::<E>()) {
            let futures: Vec<_> = list
                .iter()
                .map(|h| h(&event as &dyn Any))
                .collect();
            futures::future::join_all(futures).await;
        }
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new()
    }
}
