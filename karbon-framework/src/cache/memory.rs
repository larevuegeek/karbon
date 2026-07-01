use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use super::store::{CacheStore, StoreFuture};

struct Entry {
    data: String,
    expires_at: Instant,
}

/// In-memory [`CacheStore`] backend with per-entry TTL.
#[derive(Clone, Default)]
pub struct MemoryStore {
    store: Arc<RwLock<HashMap<String, Entry>>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// Drop expired entries (memory reclaim; `get` already ignores them).
    pub async fn cleanup(&self) {
        let now = Instant::now();
        self.store.write().await.retain(|_, e| e.expires_at > now);
    }
}

impl CacheStore for MemoryStore {
    fn get_raw<'a>(&'a self, key: &'a str) -> StoreFuture<'a, Option<String>> {
        Box::pin(async move {
            let store = self.store.read().await;
            let entry = store.get(key)?;
            if Instant::now() > entry.expires_at {
                return None;
            }
            Some(entry.data.clone())
        })
    }

    fn set_raw<'a>(&'a self, key: &'a str, value: String, ttl: Duration) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            self.store.write().await.insert(
                key.to_string(),
                Entry {
                    data: value,
                    expires_at: Instant::now() + ttl,
                },
            );
        })
    }

    fn remove<'a>(&'a self, key: &'a str) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            self.store.write().await.remove(key);
        })
    }

    fn clear<'a>(&'a self) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            self.store.write().await.clear();
        })
    }
}
