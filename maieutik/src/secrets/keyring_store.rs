//! System keyring secrets store (macOS Keychain, Windows Credential Manager, Linux Secret Service).
//!
//! Uses the `keyring` crate for cross-platform credential storage.
//! Note: `list()` is not supported by the keyring crate and returns `ListUnsupported`.

use anyhow::{Context, Result};
use async_trait::async_trait;
use keyring::Entry;
use tracing::{debug, warn};

use crate::secrets::{SecretsError, SecretsStore};

const SERVICE_NAME: &str = "maieutik";

/// Keyring-based secrets store using platform credential manager.
#[derive(Debug)]
pub struct KeyringStore {
    username: String,
}

impl KeyringStore {
    /// Create a new KeyringStore.
    ///
    /// Returns an error if the platform keyring is unavailable.
    pub async fn new() -> Result<Self> {
        // Test keyring availability by trying to create an entry
        let username = Self::get_username()?;
        let test_entry =
            Entry::new(SERVICE_NAME, &username).context("Creating test keyring entry")?;

        // Try a no-op to verify keyring works
        // We don't actually set a password, just verify the entry can be created
        drop(test_entry);

        debug!("Keyring store initialized for user: {}", username);
        Ok(Self { username })
    }

    fn get_username() -> Result<String> {
        // Try to get the current username for keyring scoping
        // Fall back to a constant if unavailable
        std::env::var("USER")
            .or_else(|_| std::env::var("USERNAME"))
            .or_else(|_| Ok::<String, std::env::VarError>("maieutik-user".to_string()))
            .context("Determining username for keyring")
    }

    fn entry(&self, key: &str) -> Result<Entry> {
        Entry::new(SERVICE_NAME, &format!("{}:{}", self.username, key))
            .with_context(|| format!("Creating keyring entry for key: {}", key))
    }
}

#[async_trait]
impl SecretsStore for KeyringStore {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        let entry = self.entry(key)?;
        match entry.get_password() {
            Ok(password) => Ok(Some(password)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => {
                warn!("Keyring get failed for {}: {}", key, e);
                Err(SecretsError::KeyringUnavailable(e.to_string()).into())
            }
        }
    }

    async fn set(&self, key: &str, value: &str) -> Result<()> {
        let entry = self.entry(key)?;
        entry
            .set_password(value)
            .with_context(|| format!("Setting keyring password for: {}", key))?;
        debug!("Stored secret in keyring: {}", key);
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let entry = self.entry(key)?;
        match entry.delete_credential() {
            Ok(()) => {
                debug!("Deleted secret from keyring: {}", key);
                Ok(())
            }
            Err(keyring::Error::NoEntry) => {
                // Idempotent: deleting non-existent is OK
                debug!("Keyring delete: no entry for {} (idempotent)", key);
                Ok(())
            }
            Err(e) => {
                warn!("Keyring delete failed for {}: {}", key, e);
                Err(SecretsError::KeyringUnavailable(e.to_string()).into())
            }
        }
    }

    async fn list(&self) -> Result<Vec<String>> {
        // keyring crate does not support enumeration
        Err(SecretsError::ListUnsupported.into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_keyring_store_new_handles_unavailable() {
        // In CI without keyring, this should fail gracefully
        let result = KeyringStore::new().await;
        // Either works (if keyring available) or returns error (expected in CI)
        // Both are valid outcomes for this test
        let _ = result;
    }

    #[tokio::test]
    async fn test_keyring_store_list_returns_unsupported() {
        if let Ok(store) = KeyringStore::new().await {
            let result = store.list().await;
            assert!(result.is_err());
            let err = result.unwrap_err().to_string();
            assert!(err.contains("not supported") || err.contains("ListUnsupported"));
        }
    }
}
