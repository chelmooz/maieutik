//! Maieutik — Local-first notes with AI
//!
//! Maieutik — local-first notes with AI.

pub mod config;
pub mod core;
pub mod llm;
pub mod speech;
pub mod mcp;

pub mod ui;
pub mod utils;

use anyhow::Result;
use crate::config::store::{config_path, load_settings, save_settings, secrets};
use crate::config::AppSettings;
use crate::core::indexer::{Embedder, VectorIndex};
use crate::core::watch::{DebouncedProcessor, FileWatcher, WatchEvent};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{info, warn};

#[derive(Debug)]
pub struct AppState {
    pub settings: Arc<RwLock<AppSettings>>,
    pub vector_index: Option<Arc<VectorIndex>>,
    pub embedder: Option<Arc<Embedder>>,
}

impl AppState {
    pub async fn new() -> Result<Self> {
        let settings = Arc::new(RwLock::new(load_settings()?));
        Ok(Self {
            settings,
            vector_index: None,
            embedder: None,
        })
    }

    pub async fn initialize_indexer(
        &mut self,
        model_path: PathBuf,
        tokenizer_path: PathBuf,
    ) -> Result<()> {
        let embedder = Arc::new(Embedder::new(&model_path, &tokenizer_path)?);
        let data_dir = config::store::data_dir();
        let vector_index = Arc::new(VectorIndex::open(&data_dir, self.settings.clone(), (*embedder).clone())?);
        vector_index.build_hnsw().await?;
        self.embedder = Some(embedder);
        self.vector_index = Some(vector_index);
        Ok(())
    }
}

pub async fn run_watcher(state: Arc<AppState>) -> Result<()> {
    let (watch_tx, watch_rx) = mpsc::channel(100);
    let (process_tx, process_rx) = mpsc::channel(100);

    let mut watcher = FileWatcher::new(state.settings.clone(), watch_tx)?;
    watcher.watch().await?;

    let processor = DebouncedProcessor::new(watch_rx, process_tx);
    let processor_handle = tokio::spawn(processor.run());

    // Process debounced events
    let state_clone = state.clone();
    let process_handle = tokio::spawn(async move {
        let mut rx = process_rx;
        while let Some(event) = rx.recv().await {
            match event {
                WatchEvent::Created(path) | WatchEvent::Modified(path) => {
                    info!("File changed: {}", path.display());
                    // TODO: re-index file
                }
                WatchEvent::Removed(path) => {
                    info!("File removed: {}", path.display());
                    if let Some(idx) = &state_clone.vector_index {
                        let _ = idx.remove_file(&path).await;
                    }
                }
                WatchEvent::Renamed { from, to } => {
                    info!("File renamed: {} -> {}", from.display(), to.display());
                    if let Some(idx) = &state_clone.vector_index {
                        let _ = idx.remove_file(&from).await;
                        // TODO: re-index at new path
                    }
                }
            }
        }
    });

    // Wait for shutdown signal
    tokio::select! {
        _ = processor_handle => {},
        _ = process_handle => {},
    }

    Ok(())
}

pub fn save_settings_if_changed(settings: &AppSettings) -> Result<()> {
    save_settings(settings)
}