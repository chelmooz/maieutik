//! Secrets Store — Cross-platform secure storage for API keys, tokens, etc.
//!
//! Provides a trait `SecretsStore` with two implementations:
//! - `FileStore`: Encrypted JSON file (dev/CI fallback, works everywhere)
//! - `KeyringStore`: System keyring (macOS Keychain, Windows Credential Manager, Linux Secret Service / KWallet)

use anyhow::Result;
use async_trait::async_trait;
use std::fmt::Debug;

/// Trait for secrets storage backends.
///
/// Implementations must be thread-safe (`Send + Sync`) and support async operations.
/// The trait is object-safe for dynamic dispatch via `Arc<dyn SecretsStore>`.
#[async_trait]
pub trait SecretsStore: Send + Sync + Debug + 'static {
    /// Get a secret by key.
    ///
    /// Returns `Ok(None)` if the key doesn't exist.
    async fn get(&self, key: &str) -> Result<Option<String>>;

    /// Set a secret (insert or update).
    async fn set(&self, key: &str, value: &str) -> Result<()>;

    /// Delete a secret by key.
    ///
    /// Returns `Ok(())` even if the key didn't exist (idempotent).
    async fn delete(&self, key: &str) -> Result<()>;

    /// List all secret keys.
    ///
    /// Note: `KeyringStore` returns `Err(SecretsError::ListUnsupported)`
    /// because system keyrings don't support enumeration.
    async fn list(&self) -> Result<Vec<String>>;
}

/// Errors specific to secrets operations.
#[derive(Debug, thiserror::Error)]
pub enum SecretsError {
    #[error("Key not found: {0}")]
    NotFound(String),

    #[error("Keyring backend unavailable: {0}")]
    KeyringUnavailable(String),

    #[error("List operation not supported by this backend")]
    ListUnsupported,

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Encryption error: {0}")]
    Encryption(String),
}

/// Default secrets directory path (`$MAIEUTIK_CONFIG_DIR/secrets.json` or platform config dir).
pub fn secrets_path() -> Result<std::path::PathBuf> {
    use crate::config::store::settings_dir;
    settings_dir().map(|d| d.join("secrets.json"))
}

/// Factory to create the default secrets store based on environment.
///
/// Priority:
/// 1. `MAIEUTIK_SECRETS_BACKEND=keyring` → `KeyringStore` (with fallback to FileStore on failure)
/// 2. `MAIEUTIK_SECRETS_BACKEND=file` → `FileStore`
/// 3. Default → `KeyringStore` with fallback to `FileStore` on failure
pub async fn default_secrets_store() -> Result<Arc<dyn SecretsStore>> {
    use std::sync::Arc;

    let backend = std::env::var("MAIEUTIK_SECRETS_BACKEND").unwrap_or_default();

    match backend.as_str() {
        "file" => Ok(Arc::new(FileStore::new().await?)),
        "keyring" => match KeyringStore::new().await {
            Ok(store) => Ok(Arc::new(store)),
            Err(e) => {
                tracing::warn!("Keyring unavailable ({}), falling back to file store", e);
                Ok(Arc::new(FileStore::new().await?))
            }
        },
        _ => {
            // Default: try keyring first, fall back to file
            match KeyringStore::new().await {
                Ok(store) => Ok(Arc::new(store)),
                Err(e) => {
                    tracing::debug!("Keyring unavailable ({}), using file store", e);
                    Ok(Arc::new(FileStore::new().await?))
                }
            }
        }
    }
}

mod file_store;
mod keyring_store;

pub use file_store::FileStore;
pub use keyring_store::KeyringStore;

use std::sync::Arc;

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::collections::HashMap;
    use tempfile::TempDir;

    /// Mock store for testing ConfigStore injection.
    #[derive(Debug)]
    pub struct MockStore {
        data: parking_lot::RwLock<HashMap<String, String>>,
    }

    impl MockStore {
        pub fn new() -> Self {
            Self {
                data: parking_lot::RwLock::new(HashMap::new()),
            }
        }
    }

    impl Default for MockStore {
        fn default() -> Self {
            Self::new()
        }
    }

    #[async_trait]
    impl SecretsStore for MockStore {
        async fn get(&self, key: &str) -> Result<Option<String>> {
            Ok(self.data.read().get(key).cloned())
        }

        async fn set(&self, key: &str, value: &str) -> Result<()> {
            self.data.write().insert(key.to_string(), value.to_string());
            Ok(())
        }

        async fn delete(&self, key: &str) -> Result<()> {
            self.data.write().remove(key);
            Ok(())
        }

        async fn list(&self) -> Result<Vec<String>> {
            Ok(self.data.read().keys().cloned().collect())
        }
    }

    #[tokio::test]
    async fn test_secrets_store_trait_object_safety() {
        let _store: Box<dyn SecretsStore> = Box::new(MockStore::new());
        // Compiles = trait is object-safe (Send + Sync + 'static)
    }

    #[tokio::test]
    async fn test_secrets_store_get_missing_returns_none() {
        let store = MockStore::new();
        let result = store.get("nonexistent").await.expect("test");
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_secrets_store_set_then_get() {
        let store = MockStore::new();
        store.set("api_key", "sk-test123").await.expect("test");
        let result = store.get("api_key").await.expect("test");
        assert_eq!(result, Some("sk-test123".to_string()));
    }

    #[tokio::test]
    async fn test_secrets_store_delete_then_get_none() {
        let store = MockStore::new();
        store.set("key", "value").await.expect("test");
        store.delete("key").await.expect("test");
        let result = store.get("key").await.expect("test");
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_secrets_store_list_keys() {
        let store = MockStore::new();
        store.set("key1", "v1").await.expect("test");
        store.set("key2", "v2").await.expect("test");
        let keys = store.list().await.expect("test");
        assert_eq!(keys.len(), 2);
        assert!(keys.contains(&"key1".to_string()));
        assert!(keys.contains(&"key2".to_string()));
    }

    #[tokio::test]
    async fn test_file_store_persists_across_instances() {
        let tmp = TempDir::new().expect("test");
        let path = tmp.path().join("secrets.json");

        // First instance writes
        {
            let store = FileStore::with_path(path.clone()).await.expect("test");
            store
                .set("persist_key", "persist_value")
                .await
                .expect("test");
        }

        // Second instance reads
        {
            let store = FileStore::with_path(path).await.expect("test");
            let result = store.get("persist_key").await.expect("test");
            assert_eq!(result, Some("persist_value".to_string()));
        }
    }

    #[tokio::test]
    async fn test_file_store_permissions_0600() {
        let tmp = TempDir::new().expect("test");
        let path = tmp.path().join("secrets.json");

        let store = FileStore::with_path(path.clone()).await.expect("test");
        store.set("test", "value").await.expect("test");

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let metadata = std::fs::metadata(&path).expect("test");
            let mode = metadata.mode() & 0o777;
            assert_eq!(
                mode, 0o600,
                "File permissions should be 0600, got {:o}",
                mode
            );
        }
    }

    #[tokio::test]
    async fn test_file_store_concurrent_access() {
        let tmp = TempDir::new().expect("test");
        let path = tmp.path().join("secrets.json");

        let store = std::sync::Arc::new(FileStore::with_path(path).await.expect("test"));
        let mut handles = vec![];

        for i in 0..10 {
            let store = store.clone();
            handles.push(tokio::spawn(async move {
                store
                    .set(&format!("key{}", i), &format!("value{}", i))
                    .await
                    .expect("test");
            }));
        }

        for h in handles {
            h.await.expect("test");
        }

        // Verify all written
        for i in 0..10 {
            let val = store.get(&format!("key{}", i)).await.expect("test");
            assert_eq!(val, Some(format!("value{}", i)));
        }
    }

    #[tokio::test]
    async fn test_file_store_empty_dir_returns_empty_list() {
        let tmp = TempDir::new().expect("test");
        let path = tmp.path().join("secrets.json");

        let store = FileStore::with_path(path).await.expect("test");
        let keys = store.list().await.expect("test");
        assert!(keys.is_empty());
    }

    #[tokio::test]
    async fn test_keyring_store_list_unsupported() {
        // KeyringStore::list() should return ListUnsupported error
        // We test the error type without needing a real keyring
        let store = KeyringStore::new().await;
        if let Ok(store) = store {
            let result = store.list().await;
            assert!(matches!(result, Err(e) if e.to_string().contains("not supported")));
        }
        // If KeyringStore::new() fails (no keyring available), that's fine too - test passes
    }

    #[tokio::test]
    async fn test_default_secrets_store_factory() {
        // Test that factory returns a working store (file fallback in CI)
        let store = default_secrets_store().await.expect("test");
        store.set("factory_test", "works").await.expect("test");
        let val = store.get("factory_test").await.expect("test");
        assert_eq!(val, Some("works".to_string()));
    }
}
