use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// Simple in-memory cache with TTL support
///
/// For production, swap this with a Redis-backed implementation.
#[derive(Clone)]
pub struct Cache {
    store: Arc<RwLock<HashMap<String, CacheEntry>>>,
    default_ttl: Duration,
}

struct CacheEntry {
    data: String, // JSON serialized
    expires_at: Instant,
}

impl Cache {
    /// Create a new cache with a default TTL
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
            default_ttl,
        }
    }

    /// Create with 5 minute default TTL
    pub fn default_five_min() -> Self {
        Self::new(Duration::from_secs(300))
    }

    /// Get a value from cache
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let store = self.store.read().await;
        let entry = store.get(key)?;

        if Instant::now() > entry.expires_at {
            return None;
        }

        serde_json::from_str(&entry.data).ok()
    }

    /// Set a value with default TTL
    pub async fn set<T: Serialize>(&self, key: &str, value: &T) {
        self.set_with_ttl(key, value, self.default_ttl).await;
    }

    /// Set a value with a custom TTL
    pub async fn set_with_ttl<T: Serialize>(&self, key: &str, value: &T, ttl: Duration) {
        if let Ok(data) = serde_json::to_string(value) {
            let mut store = self.store.write().await;
            store.insert(
                key.to_string(),
                CacheEntry {
                    data,
                    expires_at: Instant::now() + ttl,
                },
            );
        }
    }

    /// Remove a value from cache
    pub async fn remove(&self, key: &str) {
        let mut store = self.store.write().await;
        store.remove(key);
    }

    /// Clear all expired entries
    pub async fn cleanup(&self) {
        let mut store = self.store.write().await;
        let now = Instant::now();
        store.retain(|_, entry| entry.expires_at > now);
    }

    /// Clear the entire cache
    pub async fn clear(&self) {
        let mut store = self.store.write().await;
        store.clear();
    }
}
