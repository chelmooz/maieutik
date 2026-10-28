//! Settings persistence — JSON file (mode 0600 on Unix)
use std::process;


use crate::config::AppSettings;
use anyhow::{Context, Result};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use serde_json;
use std::fs;
use std::path::PathBuf;
use tracing::{debug, info, warn};

/// Returns the settings directory, creating it if needed.
///
/// Errors if the home directory cannot be determined and
/// `ProjectDirs` is unavailable.
pub fn settings_dir() -> Result<PathBuf> {
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
/// Errors if the settings directory cannot be determined.
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
    let path = config_path()?;
    if !path.exists() {
        info!("No config file at {}, using defaults", path.display());
        return Ok(AppSettings::default());
    }
    let text = fs::read_to_string(&path).with_context(|| format!("Reading {}", path.display()))?;
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
                file.version, SETTINGS_VERSION
            );
        }
    }
    debug!("Loaded settings from {}", path.display());
    Ok(file.settings)
}

pub fn save_settings(settings: &AppSettings) -> Result<()> {
    ensure_dirs()?;
    save_settings_to(&config_path()?, settings)
}

/// Écrit les réglages de façon atomique ; fichier en 0600 sur Unix.
pub fn save_settings_to(path: &std::path::Path, settings: &AppSettings) -> Result<()> {
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
    let tmp_name = format!("{}.{}.{}.{}.tmp", path.file_name().unwrap().to_string_lossy(), process::id(), counter, rand_suffix);
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
