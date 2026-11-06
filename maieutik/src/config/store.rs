//! Settings persistence — JSON file (mode 0600 on Unix)
use crate::config::AppSettings;
use crate::secrets::{default_secrets_store, SecretsStore};
use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

/// Returns the settings directory, creating it if needed.
///
/// Errors if the home directory cannot be determined and
/// `ProjectDirs` is unavailable.
pub fn settings_dir() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("MAIEUTIK_CONFIG_DIR") {
        return Ok(PathBuf::from(path));
    }
    ProjectDirs::from("org", "maieutik", "Maieutik")
        .map(|dirs| dirs.config_dir().to_path_buf())
        .or_else(|| dirs::home_dir().map(|h| h.join(".config").join("maieutik")))
        .ok_or_else(|| anyhow::anyhow!("Impossible de déterminer le répertoire de configuration : home directory introuvable"))
}

/// Returns the data directory, creating it if needed.
///
/// Errors if the home directory cannot be determined and
/// `ProjectDirs` is unavailable.
pub fn data_dir() -> Result<PathBuf> {
    ProjectDirs::from("org", "maieutik", "Maieutik")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .or_else(|| dirs::home_dir().map(|h| h.join(".local").join("share").join("maieutik")))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Impossible de déterminer le répertoire de données : home directory introuvable"
            )
        })
}

/// Returns the models directory, creating it if needed.
///
/// Errors if the data directory cannot be determined.
pub fn models_dir() -> Result<PathBuf> {
    data_dir().map(|d| d.join("models"))
}

/// ONNX embedder model file (BAAI/bge-m3, 1024-dim dense, fp32, ≈2.3 GB).
///
/// Override with `MAIEUTIK_EMBEDDER_MODEL`.
/// Errors if the models directory cannot be determined.
pub fn embedder_model_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("MAIEUTIK_EMBEDDER_MODEL") {
        return Ok(PathBuf::from(path));
    }
    models_dir().map(|d| d.join("bge-m3").join("model.onnx"))
}

/// HuggingFace `tokenizer.json` matching the embedder model (XLM-RoBERTa
/// SentencePiece/Unigram, 250k vocabulary).
///
/// Override with `MAIEUTIK_EMBEDDER_TOKENIZER`.
/// Errors if the models directory cannot be determined.
pub fn embedder_tokenizer_path() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("MAIEUTIK_EMBEDDER_TOKENIZER") {
        return Ok(PathBuf::from(path));
    }
    models_dir().map(|d| d.join("bge-m3").join("tokenizer.json"))
}

/// Optional path to `libonnxruntime.so`, used because `ort` is built with
/// `load-dynamic`. When unset, `ort` falls back to the system loader.
pub fn ort_dylib_path() -> Option<PathBuf> {
    std::env::var_os("ORT_DYLIB_PATH").map(PathBuf::from)
}

/// Returns the path to the config file.
///
/// `MAIEUTIK_CONFIG_DIR` is already honoured by [`settings_dir`]; the file is
/// always `<settings_dir>/config.json`.
pub fn config_path() -> Result<PathBuf> {
    settings_dir().map(|d| d.join("config.json"))
}

pub fn ensure_dirs() -> Result<()> {
    fs::create_dir_all(settings_dir()?)?;
    fs::create_dir_all(data_dir()?)?;
    fs::create_dir_all(models_dir()?)?;
    Ok(())
}

const SETTINGS_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct SettingsFile {
    version: u32,
    settings: AppSettings,
}

pub fn load_settings() -> Result<AppSettings> {
    ensure_dirs()?;
    load_settings_from(&config_path()?)
}

/// Load settings from an explicit file without consulting or creating user directories.
pub fn load_settings_from(path: &Path) -> Result<AppSettings> {
    if !path.exists() {
        info!("No config file at {}, using defaults", path.display());
        return Ok(AppSettings::default());
    }
    let text = fs::read_to_string(path).with_context(|| format!("Reading {}", path.display()))?;
    let file: SettingsFile = serde_json::from_str(&text)
        .with_context(|| format!("Parsing JSON from {}", path.display()))?;

    // Handle version migration
    match file.version.cmp(&SETTINGS_VERSION) {
        std::cmp::Ordering::Equal => {
            // Current version - OK
        }
        std::cmp::Ordering::Less => {
            // Old version - migrate
            warn!(
                "Config version {} is older than current {}, migrating...",
                file.version, SETTINGS_VERSION
            );
            // TODO: implement actual migration logic per version
            // For now, accept old config as-is with warning
        }
        std::cmp::Ordering::Greater => {
            // Future version - reject
            anyhow::bail!(
                "Config version {} is newer than supported {}; please update Maieutik",
                file.version,
                SETTINGS_VERSION
            );
        }
    }
    file.settings
        .validate_values()
        .with_context(|| format!("Validating settings from {}", path.display()))?;
    for folder in file.settings.missing_folders() {
        warn!("Configured folder is currently unavailable: {}", folder);
    }
    debug!("Loaded settings from {}", path.display());
    Ok(file.settings)
}

pub fn save_settings(settings: &AppSettings) -> Result<()> {
    ensure_dirs()?;
    save_settings_to_file(settings, &config_path()?)
}

/// Écrit les réglages de façon atomique ; fichier en 0600 sur Unix.
pub fn save_settings_to_file(settings: &AppSettings, path: &Path) -> Result<()> {
    use std::io::Write;
    let file = SettingsFile {
        version: SETTINGS_VERSION,
        settings: settings.clone(),
    };
    let text = serde_json::to_string_pretty(&file)?;

    // Generate unique temporary filename to avoid races:
    // config.json.<pid>.<counter>.<random>.tmp
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    use rand::Rng;
    let counter = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let rand_suffix: u64 = rand::thread_rng().gen();
    let file_name = path.file_name().ok_or_else(|| {
        anyhow::anyhow!("Configuration path has no file name: {}", path.display())
    })?;
    let tmp_name = format!(
        "{}.{}.{}.{}.tmp",
        file_name.to_string_lossy(),
        process::id(),
        counter,
        rand_suffix
    );
    let tmp = path.with_file_name(tmp_name);

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
    fs::rename(&tmp, path)?;
    debug!("Saved settings to {}", path.display());
    Ok(())
}

/// Compatibility wrapper for callers using the original argument order.
pub fn save_settings_to(path: &Path, settings: &AppSettings) -> Result<()> {
    save_settings_to_file(settings, path)
}

/// Configuration store with injected secrets backend.
///
/// This struct manages both application settings and secrets (API keys, tokens).
/// The secrets backend is injected via dependency injection (DIP), allowing
/// different implementations (KeyringStore, FileStore, mock for tests).
#[derive(Debug)]
pub struct ConfigStore {
    pub settings: Arc<RwLock<AppSettings>>,
    pub secrets: Arc<dyn SecretsStore>,
}

impl ConfigStore {
    /// Create a new ConfigStore with an optional injected secrets store.
    ///
    /// If `secrets` is `None`, the default store is created via `default_secrets_store()`,
    /// which tries keyring first and falls back to file store.
    pub async fn new(secrets: Option<Arc<dyn SecretsStore>>) -> Result<Self> {
        ensure_dirs()?;
        let settings = Arc::new(RwLock::new(load_settings()?));
        let secrets = match secrets {
            Some(s) => s,
            None => default_secrets_store().await?,
        };
        Ok(Self { settings, secrets })
    }

    /// Save settings to disk (atomic, 0600).
    pub async fn save_settings(&self) -> Result<()> {
        let settings = self.settings.read().await.clone();
        save_settings_to_file(&settings, &config_path()?)
    }

    /// Get a secret by key.
    pub async fn get_secret(&self, key: &str) -> Result<Option<String>> {
        self.secrets.get(key).await
    }

    /// Set a secret (insert or update).
    pub async fn set_secret(&self, key: &str, value: &str) -> Result<()> {
        self.secrets.set(key, value).await
    }

    /// Delete a secret by key.
    pub async fn delete_secret(&self, key: &str) -> Result<()> {
        self.secrets.delete(key).await
    }

    /// List all secret keys (may return `SecretsError::ListUnsupported` for keyring).
    pub async fn list_secrets(&self) -> Result<Vec<String>> {
        self.secrets.list().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::tests::MockStore;
    use std::sync::Arc;

    #[tokio::test]
    async fn test_config_store_uses_injected_secrets() {
        let mock = Arc::new(MockStore::new());
        let store = ConfigStore::new(Some(mock.clone())).await.expect("test");

        store
            .set_secret("test_key", "test_value")
            .await
            .expect("test");
        let val = store.get_secret("test_key").await.expect("test");
        assert_eq!(val, Some("test_value".to_string()));
    }

    #[tokio::test]
    async fn test_config_store_falls_back_to_file_store() {
        // Without MAIEUTIK_SECRETS_BACKEND=keyring, should use FileStore
        let store = ConfigStore::new(None).await.expect("test");

        store
            .set_secret("fallback_test", "works")
            .await
            .expect("test");
        let val = store.get_secret("fallback_test").await.expect("test");
        assert_eq!(val, Some("works".to_string()));
    }
}
