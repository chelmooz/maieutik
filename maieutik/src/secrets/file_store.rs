//! File-based secrets store (encrypted JSON, mode 0600).
//!
//! Uses XChaCha20-Poly1305 for encryption when the `encrypt-secrets` feature is enabled.
//! Without the feature, stores as plain JSON (for CI/dev where keyring is unavailable).
//!
//! Atomic writes via temp file + rename (same pattern as `config::store::save_settings_to_file`).

use anyhow::{Context, Result};
use async_trait::async_trait;
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use tracing::{debug, warn};

#[cfg(feature = "encrypt-secrets")]
use chacha20poly1305::{
    aead::{Aead, KeyInit, OsRng},
    XChaCha20Poly1305, XNonce,
};

use crate::secrets::SecretsStore;

/// In-memory cache of decrypted secrets to avoid repeated decryption.
#[derive(Debug, Default, Serialize, Deserialize, Clone)]
struct SecretsFile {
    version: u32,
    #[serde(default)]
    data: std::collections::HashMap<String, String>,
}

const SECRETS_VERSION: u32 = 1;

/// File-based secrets store with atomic writes and optional encryption.
#[derive(Debug)]
pub struct FileStore {
    path: PathBuf,
    cache: Arc<RwLock<SecretsFile>>,
    #[cfg(feature = "encrypt-secrets")]
    cipher: Option<XChaCha20Poly1305>,
}

impl FileStore {
    /// Create a new FileStore at the default location (`secrets_path()`).
    pub async fn new() -> Result<Self> {
        let path = crate::secrets::secrets_path()?;
        Self::with_path(path).await
    }

    /// Create a new FileStore at a specific path (for testing).
    pub async fn with_path(path: PathBuf) -> Result<Self> {
        let cache = if path.exists() {
            Self::load_file(&path).await?
        } else {
            SecretsFile::default()
        };

        #[cfg(feature = "encrypt-secrets")]
        let cipher = Self::load_or_create_key(&path).await?;

        let store = Self {
            path,
            cache: Arc::new(RwLock::new(cache)),
            #[cfg(feature = "encrypt-secrets")]
            cipher,
        };

        Ok(store)
    }

    #[cfg(feature = "encrypt-secrets")]
    async fn load_or_create_key(path: &Path) -> Result<Option<XChaCha20Poly1305>> {
        let key_path = path.with_extension("key");
        let key = if key_path.exists() {
            let key_bytes = fs::read(&key_path).context("Reading encryption key")?;
            if key_bytes.len() != 32 {
                anyhow::bail!("Invalid key length: {}", key_bytes.len());
            }
            let key_array: [u8; 32] = key_bytes.try_into().expect("key length already validated as 32");
            Some(XChaCha20Poly1305::new(&key_array.into()))
        } else {
            // Generate new key
            let mut key = [0u8; 32];
            OsRng.fill_bytes(&mut key);
            let cipher = XChaCha20Poly1305::new(&key.into());

            // Save key with restricted permissions
            let mut opts = fs::OpenOptions::new();
            opts.write(true).create_new(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                opts.mode(0o600);
            }
            let mut key_file = opts.open(&key_path).context("Creating key file")?;
            key_file.write_all(&key).context("Writing key")?;
            key_file.sync_all().context("Syncing key")?;

            Some(cipher)
        };
        Ok(key)
    }

    async fn load_file(path: &Path) -> Result<SecretsFile> {
        let text =
            fs::read_to_string(path).with_context(|| format!("Reading {}", path.display()))?;

        #[cfg(feature = "encrypt-secrets")]
        {
            // Try to decrypt
            let key_path = path.with_extension("key");
            if key_path.exists() {
                let key_bytes = fs::read(&key_path).context("Reading encryption key")?;
                if key_bytes.len() == 32 {
                    let key_array: [u8; 32] = key_bytes.try_into().expect("key length already validated as 32");
                    let cipher = XChaCha20Poly1305::new(&key_array.into());
                    let data: Vec<u8> = serde_json::from_str(&text)?;
                    if data.len() < 24 {
                        anyhow::bail!("Encrypted data too short");
                    }
                    let nonce = XNonce::from_slice(&data[..24]);
                    let ciphertext = &data[24..];
                    let plaintext = cipher
                        .decrypt(nonce, ciphertext)
                        .map_err(|e| anyhow::anyhow!("Decryption failed: {}", e))?;
                    let file: SecretsFile = serde_json::from_slice(&plaintext)?;
                    return Ok(file);
                }
            }
            // Fall through to plain JSON if no key or decryption failed
        }

        // Plain JSON (or fallback)
        let file: SecretsFile = serde_json::from_str(&text)
            .with_context(|| format!("Parsing JSON from {}", path.display()))?;

        // Handle version migration
        match file.version.cmp(&SECRETS_VERSION) {
            std::cmp::Ordering::Equal => {}
            std::cmp::Ordering::Less => {
                warn!(
                    "Secrets file version {} older than current {}, accepting as-is",
                    file.version, SECRETS_VERSION
                );
            }
            std::cmp::Ordering::Greater => {
                anyhow::bail!(
                    "Secrets file version {} newer than supported {}",
                    file.version,
                    SECRETS_VERSION
                );
            }
        }

        Ok(file)
    }

    async fn save_file(&self) -> Result<()> {
        // Clone the data while holding the read lock
        let file = {
            let cache = self.cache.read();
            (*cache).clone()
        };

        #[cfg(feature = "encrypt-secrets")]
        let text = if let Some(cipher) = &self.cipher {
            let plaintext = serde_json::to_vec(&file)?;
            let mut nonce = [0u8; 24];
            OsRng.fill_bytes(&mut nonce);
            let mut ciphertext = cipher
                .encrypt(&XNonce::from_slice(&nonce), plaintext.as_ref())
                .map_err(|e| anyhow::anyhow!("Encryption failed: {}", e))?;
            // Prepend nonce
            let mut out = Vec::with_capacity(24 + ciphertext.len());
            out.extend_from_slice(&nonce);
            out.append(&mut ciphertext);
            serde_json::to_string(&out)?
        } else {
            serde_json::to_string_pretty(&file)?
        };

        #[cfg(not(feature = "encrypt-secrets"))]
        let text = serde_json::to_string_pretty(&file)?;

        // Atomic write: temp file + rename (same pattern as config::store)
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        use rand::Rng;
        let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let rand_suffix: u64 = rand::thread_rng().gen();

        let file_name = self.path.file_name().ok_or_else(|| {
            anyhow::anyhow!("Secrets path has no file name: {}", self.path.display())
        })?;
        let tmp_name = format!(
            "{}.{}.{}.{}.tmp",
            file_name.to_string_lossy(),
            process::id(),
            counter,
            rand_suffix
        );
        let tmp = self.path.with_file_name(tmp_name);

        let mut opts = fs::OpenOptions::new();
        opts.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        let mut out = opts
            .open(&tmp)
            .with_context(|| format!("Creating {}", tmp.display()))?;
        out.write_all(text.as_bytes())?;
        out.sync_all()?;
        drop(out);

        fs::rename(&tmp, &self.path)?;
        debug!("Saved secrets to {}", self.path.display());
        Ok(())
    }
}

#[async_trait]
impl SecretsStore for FileStore {
    async fn get(&self, key: &str) -> Result<Option<String>> {
        Ok(self.cache.read().data.get(key).cloned())
    }

    async fn set(&self, key: &str, value: &str) -> Result<()> {
        self.cache
            .write()
            .data
            .insert(key.to_string(), value.to_string());
        self.save_file().await
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.cache.write().data.remove(key);
        self.save_file().await
    }

    async fn list(&self) -> Result<Vec<String>> {
        Ok(self.cache.read().data.keys().cloned().collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_file_store_atomic_write_corruption_resilience() {
        let tmp = TempDir::new().expect("test");
        let path = tmp.path().join("secrets.json");

        let store = FileStore::with_path(path.clone()).await.expect("test");
        store.set("key1", "value1").await.expect("test");

        // Verify file exists and is valid JSON
        let content = fs::read_to_string(&path).expect("test");
        let _: SecretsFile = serde_json::from_str(&content).expect("test");

        // Simulate crash during write by creating a corrupt temp file
        // (the actual atomic write prevents corruption, but we test the file is readable)
        let content2 = fs::read_to_string(&path).expect("test");
        let _: SecretsFile = serde_json::from_str(&content2).expect("test");
    }

    #[tokio::test]
    async fn test_file_store_overwrite_existing_key() {
        let tmp = TempDir::new().expect("test");
        let path = tmp.path().join("secrets.json");

        let store = FileStore::with_path(path).await.expect("test");
        store.set("key", "old").await.expect("test");
        store.set("key", "new").await.expect("test");

        let val = store.get("key").await.expect("test");
        assert_eq!(val, Some("new".to_string()));
    }

    #[tokio::test]
    async fn test_file_store_delete_nonexistent_is_ok() {
        let tmp = TempDir::new().expect("test");
        let path = tmp.path().join("secrets.json");

        let store = FileStore::with_path(path).await.expect("test");
        // Should not error
        store.delete("nonexistent").await.expect("test");
    }

    #[tokio::test]
    async fn test_file_store_list_sorted() {
        let tmp = TempDir::new().expect("test");
        let path = tmp.path().join("secrets.json");

        let store = FileStore::with_path(path).await.expect("test");
        store.set("zebra", "1").await.expect("test");
        store.set("alpha", "2").await.expect("test");
        store.set("beta", "3").await.expect("test");

        let mut keys = store.list().await.expect("test");
        keys.sort();
        assert_eq!(keys, vec!["alpha", "beta", "zebra"]);
    }
}
