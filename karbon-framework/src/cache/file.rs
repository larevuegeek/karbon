use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::store::{CacheStore, StoreFuture};

/// Filesystem-backed [`CacheStore`]. Each key maps to a JSON file (named by the
/// SHA-256 of the key) holding the value and a wall-clock expiry. Survives
/// process restarts and is shareable across processes on the same host.
#[derive(Clone)]
pub struct FileStore {
    dir: PathBuf,
}

#[derive(Serialize, Deserialize)]
struct FileEntry {
    expires_at: u64, // unix seconds
    data: String,
}

impl FileStore {
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        let dir = dir.into();
        let _ = std::fs::create_dir_all(&dir);
        Self { dir }
    }

    fn path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{}.json", key_hash(key)))
    }
}

fn key_hash(key: &str) -> String {
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(key.as_bytes());
    let mut s = String::with_capacity(64);
    for b in digest {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl CacheStore for FileStore {
    fn get_raw<'a>(&'a self, key: &'a str) -> StoreFuture<'a, Option<String>> {
        let path = self.path(key);
        Box::pin(async move {
            let raw = tokio::fs::read_to_string(&path).await.ok()?;
            let entry: FileEntry = serde_json::from_str(&raw).ok()?;
            if now_unix() > entry.expires_at {
                let _ = tokio::fs::remove_file(&path).await;
                return None;
            }
            Some(entry.data)
        })
    }

    fn set_raw<'a>(&'a self, key: &'a str, value: String, ttl: Duration) -> StoreFuture<'a, ()> {
        let path = self.path(key);
        Box::pin(async move {
            let entry = FileEntry {
                expires_at: now_unix() + ttl.as_secs(),
                data: value,
            };
            if let Ok(json) = serde_json::to_string(&entry) {
                let _ = tokio::fs::write(&path, json).await;
            }
        })
    }

    fn remove<'a>(&'a self, key: &'a str) -> StoreFuture<'a, ()> {
        let path = self.path(key);
        Box::pin(async move {
            let _ = tokio::fs::remove_file(&path).await;
        })
    }

    fn clear<'a>(&'a self) -> StoreFuture<'a, ()> {
        let dir = self.dir.clone();
        Box::pin(async move {
            if let Ok(mut entries) = tokio::fs::read_dir(&dir).await {
                while let Ok(Some(entry)) = entries.next_entry().await {
                    let p = entry.path();
                    if p.extension().is_some_and(|e| e == "json") {
                        let _ = tokio::fs::remove_file(&p).await;
                    }
                }
            }
        })
    }
}
