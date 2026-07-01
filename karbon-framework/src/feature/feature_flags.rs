use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// A single feature flag with optional metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureFlag {
    pub name: String,
    pub enabled: bool,
    pub description: String,
    pub updated_at: DateTime<Utc>,
}

/// In-memory runtime feature flag manager.
///
/// ```ignore
/// let flags = FeatureFlags::new();
///
/// // Register flags at startup
/// flags.register("dark_mode", true, "Enable dark mode UI").await;
/// flags.register("beta_search", false, "New search algorithm").await;
///
/// // Check in handlers
/// if flags.is_enabled("dark_mode").await {
///     // show dark mode
/// }
///
/// // Toggle at runtime (e.g., from an admin endpoint)
/// flags.toggle("beta_search").await;
/// ```
#[derive(Clone)]
pub struct FeatureFlags {
    store: Arc<RwLock<HashMap<String, FeatureFlag>>>,
}

impl FeatureFlags {
    pub fn new() -> Self {
        Self {
            store: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a feature flag with an initial state
    pub async fn register(&self, name: &str, enabled: bool, description: &str) {
        self.store.write().await.insert(
            name.to_string(),
            FeatureFlag {
                name: name.to_string(),
                enabled,
                description: description.to_string(),
                updated_at: Utc::now(),
            },
        );
    }

    /// Check if a feature is enabled (returns false if not registered)
    pub async fn is_enabled(&self, name: &str) -> bool {
        self.store.read().await.get(name).is_some_and(|f| f.enabled)
    }

    /// Enable a feature flag
    pub async fn enable(&self, name: &str) -> bool {
        self.set(name, true).await
    }

    /// Disable a feature flag
    pub async fn disable(&self, name: &str) -> bool {
        self.set(name, false).await
    }

    /// Toggle a feature flag, returns the new state
    pub async fn toggle(&self, name: &str) -> Option<bool> {
        let mut store = self.store.write().await;
        if let Some(flag) = store.get_mut(name) {
            flag.enabled = !flag.enabled;
            flag.updated_at = Utc::now();
            Some(flag.enabled)
        } else {
            None
        }
    }

    /// Set a feature flag to a specific state
    pub async fn set(&self, name: &str, enabled: bool) -> bool {
        let mut store = self.store.write().await;
        if let Some(flag) = store.get_mut(name) {
            flag.enabled = enabled;
            flag.updated_at = Utc::now();
            true
        } else {
            false
        }
    }

    /// Get a single feature flag
    pub async fn get(&self, name: &str) -> Option<FeatureFlag> {
        self.store.read().await.get(name).cloned()
    }

    /// List all feature flags
    pub async fn list(&self) -> Vec<FeatureFlag> {
        self.store.read().await.values().cloned().collect()
    }

    /// Remove a feature flag
    pub async fn remove(&self, name: &str) -> bool {
        self.store.write().await.remove(name).is_some()
    }

    /// Register multiple flags at once from tuples
    pub async fn register_many(&self, flags: &[(&str, bool, &str)]) {
        let mut store = self.store.write().await;
        for (name, enabled, description) in flags {
            store.insert(
                name.to_string(),
                FeatureFlag {
                    name: name.to_string(),
                    enabled: *enabled,
                    description: description.to_string(),
                    updated_at: Utc::now(),
                },
            );
        }
    }
}

impl Default for FeatureFlags {
    fn default() -> Self {
        Self::new()
    }
}
