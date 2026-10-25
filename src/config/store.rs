//! Settings persistence — JSON file (mode 0600 on Unix)

use anyhow::{Context, Result};
use crate::config::AppSettings;
use serde::{Serialize, Deserialize};
use directories::ProjectDirs;
use serde_json;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use tracing::{debug, info, warn};

static SETTINGS_DIR: OnceLock<PathBuf> = OnceLock::new();

pub fn settings_dir() -> &'static PathBuf {
    SETTINGS_DIR.get_or_init(|| {
        ProjectDirs::from("org", "maieutik", "Maieutik")
            .map(|dirs| dirs.config_dir().to_path_buf())
            .unwrap_or_else(|| dirs::home_dir().unwrap().join(".config").join("maieutik"))
    })
}

pub fn data_dir() -> PathBuf {
    ProjectDirs::from("org", "maieutik", "Maieutik")
        .map(|dirs| dirs.data_dir().to_path_buf())
        .unwrap_or_else(|| dirs::home_dir().unwrap().join(".local").join("share").join("maieutik"))
}

pub fn models_dir() -> PathBuf {
    data_dir().join("models")
}

/// ONNX embedder model file (BAAI/bge-m3, 1024-dim dense, fp32, ≈2.3 GB).
///
/// Override with `MAIEUTIK_EMBEDDER_MODEL`.
pub fn embedder_model_path() -> PathBuf {
    std::env::var_os("MAIEUTIK_EMBEDDER_MODEL")
        .map(PathBuf::from)
        .unwrap_or_else(|| models_dir().join("bge-m3").join("model.onnx"))
}

/// HuggingFace `tokenizer.json` matching the embedder model (XLM-RoBERTa
/// SentencePiece/Unigram, 250k vocabulary).
///
/// Override with `MAIEUTIK_EMBEDDER_TOKENIZER`.
pub fn embedder_tokenizer_path() -> PathBuf {
    std::env::var_os("MAIEUTIK_EMBEDDER_TOKENIZER")
        .map(PathBuf::from)
        .unwrap_or_else(|| models_dir().join("bge-m3").join("tokenizer.json"))
}

/// Optional path to `libonnxruntime.so`, used because `ort` is built with
/// `load-dynamic`. When unset, `ort` falls back to the system loader.
pub fn ort_dylib_path() -> Option<PathBuf> {
    std::env::var_os("ORT_DYLIB_PATH").map(PathBuf::from)
}

pub fn config_path() -> PathBuf {
    settings_dir().join("config.json")
}

pub fn ensure_dirs() -> Result<()> {
    fs::create_dir_all(settings_dir())?;
    fs::create_dir_all(data_dir())?;
    fs::create_dir_all(models_dir())?;
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
    let path = config_path();
    if !path.exists() {
        info!("No config file at {}, using defaults", path.display());
        return Ok(AppSettings::default());
    }
    let text = fs::read_to_string(&path)
        .with_context(|| format!("Reading {}", path.display()))?;
    let file: SettingsFile = serde_json::from_str(&text)
        .with_context(|| format!("Parsing JSON from {}", path.display()))?;
    if file.version != SETTINGS_VERSION {
        warn!("Config version mismatch (file={}, expected={}), migrating...", file.version, SETTINGS_VERSION);
        // TODO: implement migration
    }
    debug!("Loaded settings from {}", path.display());
    Ok(file.settings)
}

pub fn save_settings(settings: &AppSettings) -> Result<()> {
    ensure_dirs()?;
    save_settings_to(&config_path(), settings)
}

/// Écrit les réglages de façon atomique ; fichier en 0600 sur Unix.
pub fn save_settings_to(path: &std::path::Path, settings: &AppSettings) -> Result<()> {
    use std::io::Write;
    let file = SettingsFile {
        version: SETTINGS_VERSION,
        settings: settings.clone(),
    };
    let text = serde_json::to_string_pretty(&file)?;
    let tmp = path.with_extension("json.tmp");
    // Un .tmp résiduel garderait ses anciennes permissions : on le retire.
    let _ = fs::remove_file(&tmp);
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