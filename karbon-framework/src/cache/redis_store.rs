//! Redis-backed [`CacheStore`] (feature `redis`).

use std::time::Duration;

use redis::AsyncCommands;
use redis::aio::ConnectionManager;

use super::store::{CacheStore, StoreFuture};

/// A [`CacheStore`] backed by Redis. Cheap to clone (shares the connection
/// manager). Keys are namespaced with `prefix` so [`RedisStore::clear`] only
/// affects this cache's keys.
#[derive(Clone)]
pub struct RedisStore {
    conn: ConnectionManager,
    prefix: String,
}

impl RedisStore {
    /// Connect to Redis at `url` (e.g. `redis://127.0.0.1/`).
    pub async fn connect(url: &str) -> anyhow::Result<Self> {
        Self::connect_with_prefix(url, "karbon:").await
    }

    /// Connect with a custom key prefix.
    pub async fn connect_with_prefix(url: &str, prefix: &str) -> anyhow::Result<Self> {
        let client = redis::Client::open(url)?;
        let conn = ConnectionManager::new(client).await?;
        Ok(Self {
            conn,
            prefix: prefix.to_string(),
        })
    }

    fn k(&self, key: &str) -> String {
        format!("{}{}", self.prefix, key)
    }
}

impl CacheStore for RedisStore {
    fn get_raw<'a>(&'a self, key: &'a str) -> StoreFuture<'a, Option<String>> {
        Box::pin(async move {
            let mut conn = self.conn.clone();
            conn.get::<_, Option<String>>(self.k(key))
                .await
                .ok()
                .flatten()
        })
    }

    fn set_raw<'a>(&'a self, key: &'a str, value: String, ttl: Duration) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            let mut conn = self.conn.clone();
            let secs = ttl.as_secs().max(1);
            let _: Result<(), _> = conn.set_ex(self.k(key), value, secs).await;
        })
    }

    fn remove<'a>(&'a self, key: &'a str) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            let mut conn = self.conn.clone();
            let _: Result<(), _> = conn.del(self.k(key)).await;
        })
    }

    fn clear<'a>(&'a self) -> StoreFuture<'a, ()> {
        Box::pin(async move {
            let mut conn = self.conn.clone();
            let pattern = format!("{}*", self.prefix);
            // SCAN + DEL so we only touch this cache's namespace (not FLUSHDB).
            let mut iter = match conn.scan_match::<_, String>(pattern).await {
                Ok(it) => it,
                Err(_) => return,
            };
            let mut keys = Vec::new();
            while let Some(k) = iter.next_item().await {
                keys.push(k);
            }
            if !keys.is_empty() {
                let mut conn = self.conn.clone();
                let _: Result<(), _> = conn.del(keys).await;
            }
        })
    }
}
