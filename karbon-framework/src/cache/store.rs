use serde::{Serialize, de::DeserializeOwned};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use super::{FileStore, MemoryStore};

/// Boxed future returned by [`CacheStore`] methods (keeps the trait object-safe).
pub type StoreFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// A pluggable cache backend storing raw (already-serialized) string values.
///
/// Implementations: [`MemoryStore`], [`FileStore`]. Implement this trait to add
/// your own (e.g. Redis).
pub trait CacheStore: Send + Sync {
    /// Fetch a raw value, or `None` if missing/expired.
    fn get_raw<'a>(&'a self, key: &'a str) -> StoreFuture<'a, Option<String>>;
    /// Store a raw value with a time-to-live.
    fn set_raw<'a>(&'a self, key: &'a str, value: String, ttl: Duration) -> StoreFuture<'a, ()>;
    /// Remove a key.
    fn remove<'a>(&'a self, key: &'a str) -> StoreFuture<'a, ()>;
    /// Remove every entry.
    fn clear<'a>(&'a self) -> StoreFuture<'a, ()>;
}

/// Typed cache facade over a [`CacheStore`] backend (memory by default).
///
/// ```ignore
/// let cache = Cache::new(Duration::from_secs(300));        // in-memory
/// let cache = Cache::file("./cache", Duration::from_secs(300)); // filesystem
///
/// cache.set("user:1", &user).await;
/// let user: Option<User> = cache.get("user:1").await;
///
/// // cache-aside: compute once, then serve from cache
/// let stats = cache.remember("stats", Duration::from_secs(60), || async {
///     expensive_query().await
/// }).await?;
/// ```
#[derive(Clone)]
pub struct Cache {
    store: Arc<dyn CacheStore>,
    default_ttl: Duration,
}

impl Cache {
    /// In-memory cache with the given default TTL.
    pub fn new(default_ttl: Duration) -> Self {
        Self {
            store: Arc::new(MemoryStore::new()),
            default_ttl,
        }
    }

    /// In-memory cache with a 5-minute default TTL.
    pub fn default_five_min() -> Self {
        Self::new(Duration::from_secs(300))
    }

    /// Filesystem-backed cache rooted at `dir`.
    pub fn file(dir: impl Into<PathBuf>, default_ttl: Duration) -> Self {
        Self {
            store: Arc::new(FileStore::new(dir)),
            default_ttl,
        }
    }

    /// Cache over a custom [`CacheStore`] backend.
    pub fn with_store(store: Arc<dyn CacheStore>, default_ttl: Duration) -> Self {
        Self { store, default_ttl }
    }

    /// Redis-backed cache (feature `redis`).
    #[cfg(feature = "redis")]
    pub async fn redis(url: &str, default_ttl: Duration) -> anyhow::Result<Self> {
        let store = super::RedisStore::connect(url).await?;
        Ok(Self {
            store: Arc::new(store),
            default_ttl,
        })
    }

    /// Get and deserialize a value.
    pub async fn get<T: DeserializeOwned>(&self, key: &str) -> Option<T> {
        let raw = self.store.get_raw(key).await?;
        serde_json::from_str(&raw).ok()
    }

    /// Set a value with the default TTL.
    pub async fn set<T: Serialize>(&self, key: &str, value: &T) {
        self.set_with_ttl(key, value, self.default_ttl).await;
    }

    /// Set a value with a custom TTL.
    pub async fn set_with_ttl<T: Serialize>(&self, key: &str, value: &T, ttl: Duration) {
        if let Ok(data) = serde_json::to_string(value) {
            self.store.set_raw(key, data, ttl).await;
        }
    }

    /// Remove a key.
    pub async fn remove(&self, key: &str) {
        self.store.remove(key).await;
    }

    /// Alias for [`Cache::remove`].
    pub async fn forget(&self, key: &str) {
        self.remove(key).await;
    }

    /// Clear the entire cache.
    pub async fn clear(&self) {
        self.store.clear().await;
    }

    /// Cache-aside: return the cached value, or compute it with `f`, store it
    /// (with `ttl`) and return it.
    pub async fn remember<T, F, Fut>(&self, key: &str, ttl: Duration, f: F) -> anyhow::Result<T>
    where
        T: Serialize + DeserializeOwned,
        F: FnOnce() -> Fut,
        Fut: Future<Output = anyhow::Result<T>>,
    {
        if let Some(v) = self.get::<T>(key).await {
            return Ok(v);
        }
        let value = f().await?;
        self.set_with_ttl(key, &value, ttl).await;
        Ok(value)
    }

    /// Store a value associated with one or more **tags**, so it can later be
    /// invalidated as a group with [`Cache::invalidate_tag`]. Tag indexes are
    /// kept in the same backend, so this works with any [`CacheStore`].
    pub async fn set_tagged<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        ttl: Duration,
        tags: &[&str],
    ) {
        self.set_with_ttl(key, value, ttl).await;
        for tag in tags {
            let index_key = tag_index_key(tag);
            let mut keys: Vec<String> = self.get(&index_key).await.unwrap_or_default();
            if !keys.iter().any(|k| k == key) {
                keys.push(key.to_string());
            }
            self.set_with_ttl(&index_key, &keys, ttl).await;
        }
    }

    /// Remove every key tagged with `tag` (and the tag index itself).
    pub async fn invalidate_tag(&self, tag: &str) {
        let index_key = tag_index_key(tag);
        if let Some(keys) = self.get::<Vec<String>>(&index_key).await {
            for k in keys {
                self.remove(&k).await;
            }
        }
        self.remove(&index_key).await;
    }
}

fn tag_index_key(tag: &str) -> String {
    format!("__karbon_tag__:{tag}")
}
