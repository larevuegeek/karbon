mod file;
mod memory;
#[cfg(feature = "redis")]
mod redis_store;
mod store;

pub use file::FileStore;
pub use memory::MemoryStore;
#[cfg(feature = "redis")]
pub use redis_store::RedisStore;
pub use store::{Cache, CacheStore, StoreFuture};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn memory_set_get_remove() {
        let cache = Cache::new(Duration::from_secs(60));
        cache.set("k", &vec![1, 2, 3]).await;
        let v: Option<Vec<i32>> = cache.get("k").await;
        assert_eq!(v, Some(vec![1, 2, 3]));
        cache.remove("k").await;
        assert_eq!(cache.get::<Vec<i32>>("k").await, None);
    }

    #[tokio::test]
    async fn memory_ttl_expires() {
        let cache = Cache::new(Duration::from_secs(60));
        cache
            .set_with_ttl("k", &"v", Duration::from_millis(10))
            .await;
        tokio::time::sleep(Duration::from_millis(25)).await;
        assert_eq!(cache.get::<String>("k").await, None);
    }

    #[tokio::test]
    async fn remember_computes_once() {
        let cache = Cache::new(Duration::from_secs(60));
        let calls = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));

        for _ in 0..3 {
            let c = calls.clone();
            let v: i32 = cache
                .remember("k", Duration::from_secs(60), || async move {
                    c.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    Ok(42)
                })
                .await
                .unwrap();
            assert_eq!(v, 42);
        }
        assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn tagged_invalidation() {
        let cache = Cache::new(Duration::from_secs(60));
        cache
            .set_tagged("post:1", &"a", Duration::from_secs(60), &["posts"])
            .await;
        cache
            .set_tagged("post:2", &"b", Duration::from_secs(60), &["posts"])
            .await;
        cache.set("other", &"c").await;

        cache.invalidate_tag("posts").await;
        assert_eq!(cache.get::<String>("post:1").await, None);
        assert_eq!(cache.get::<String>("post:2").await, None);
        assert_eq!(cache.get::<String>("other").await, Some("c".to_string()));
    }

    #[tokio::test]
    async fn file_backend_roundtrip() {
        let dir = std::env::temp_dir().join(format!("karbon-cache-test-{}", std::process::id()));
        let cache = Cache::file(&dir, Duration::from_secs(60));
        cache.set("hello", &"world").await;
        assert_eq!(
            cache.get::<String>("hello").await,
            Some("world".to_string())
        );
        cache.clear().await;
        assert_eq!(cache.get::<String>("hello").await, None);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
