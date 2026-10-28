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
use crate::config::store::{load_settings, save_settings};
use crate::config::AppSettings;
use crate::core::indexer::{Embedder, VectorIndex};
use crate::core::scan::index_path;
use crate::core::watch::{
    is_extension_allowed, is_within_size_limit, DebouncedProcessor, FileWatcher, WatchEvent,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, info, warn};

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
        let vector_index = Arc::new(VectorIndex::open(&data_dir, (*embedder).clone())?);
        vector_index.build_hnsw().await?;
        self.embedder = Some(embedder);
        self.vector_index = Some(vector_index);
        Ok(())
    }
}

pub async fn run_watcher(state: Arc<AppState>) -> Result<()> {
    run_watcher_with_shutdown(state, std::future::pending()).await
}

/// Variante arrêtable de [`run_watcher`] : au signal, la source est coupée,
/// les événements encore en attente sont vidés (jamais perdus) et la tâche
/// rend la main une fois les deux boucles terminées.
pub async fn run_watcher_with_shutdown<F>(state: Arc<AppState>, shutdown: F) -> Result<()>
where
    F: std::future::Future<Output = ()>,
{
    let (watch_tx, watch_rx) = mpsc::channel(100);
    let (process_tx, process_rx) = mpsc::channel(100);

    let mut watcher = FileWatcher::new(state.settings.clone(), watch_tx.clone())?;
    watcher.watch().await?;

    let processor = DebouncedProcessor::new(watch_rx, process_tx);
    let mut processor_handle = tokio::spawn(processor.run());

    // Process debounced events
    let state_clone = state.clone();
    let mut process_handle = tokio::spawn(async move {
        let mut rx = process_rx;
        while let Some(event) = rx.recv().await {
            match event {
                WatchEvent::Created(path) | WatchEvent::Modified(path) => {
                    info!("File changed: {}", path.display());
                    reindex(&state_clone, &path).await;
                }
                WatchEvent::Removed(path) => {
                    info!("File removed: {}", path.display());
                    if let Some(idx) = &state_clone.vector_index {
                        if let Err(e) = idx.remove_file(&path).await {
                            warn!("remove failed {}: {e:#}", path.display());
                        }
                    }
                }
                WatchEvent::Renamed { from, to } => {
                    info!("File renamed: {} -> {}", from.display(), to.display());
                    if let Some(idx) = &state_clone.vector_index {
                        if let Err(e) = idx.remove_file(&from).await {
                            warn!("remove (rename source) failed {}: {e:#}", from.display());
                        }
                    }
                    reindex(&state_clone, &to).await;
                }
            }
        }
    });

    // Attente de l'arrêt demandé (ou d'une fin spontanée des boucles).
    tokio::select! {
        _ = shutdown => info!("arrêt du watcher demandé"),
        _ = &mut processor_handle => warn!("le débounce s'est arrêté de lui-même"),
        _ = &mut process_handle => warn!("le traitement des événements s'est arrêté de lui-même"),
    }

    // Arrêt ordonné : le watcher détient une copie du Sender, on le lâche en
    // premier. Le débounce voit alors son canal fermé, vide ce qui reste en
    // attente et rend la main ; sa fermeture libère le canal de traitement et
    // la boucle de réindexation termine à son tour.
    drop(watcher);
    drop(watch_tx);
    if !processor_handle.is_finished() {
        if let Err(e) = processor_handle.await {
            warn!("attente du débounce : {e}");
        }
    }
    if !process_handle.is_finished() {
        if let Err(e) = process_handle.await {
            warn!("attente du traitement des événements : {e}");
        }
    }

    Ok(())
}

/// Réindexe un fichier suite à un événement du watcher. Validation d'abord
/// (existence, extension, taille — fail-closed : un fichier à métadonnées
/// illisibles est ignoré et signalé, jamais accepté en silence), puis
/// remplacement atomique des enregistrements et reconstruction HNSW.
/// Un fichier vidé de son texte purge ses anciens enregistrements ; une
/// erreur d'extraction ne supprime jamais l'ancien contenu.
async fn reindex(state: &AppState, path: &Path) {
    if !path.is_file() {
        debug!("plus un fichier ordinaire, ignoré : {}", path.display());
        return;
    }
    let allowed = {
        let settings = state.settings.read().await;
        if !is_extension_allowed(path, &settings) {
            false
        } else {
            match is_within_size_limit(path, &settings) {
                Ok(within) => within,
                Err(e) => {
                    warn!("taille illisible, ignoré {} : {e:#}", path.display());
                    return;
                }
            }
        }
    };
    if !allowed {
        debug!("extension ou taille hors limites, ignoré : {}", path.display());
        return;
    }
    let Some(idx) = &state.vector_index else {
        debug!("index indisponible, impossible de réindexer : {}", path.display());
        return;
    };
    match index_path(idx, path).await {
        Ok(true) => info!("réindexé : {}", path.display()),
        Ok(false) => info!("sans texte, enregistrements purgés : {}", path.display()),
        Err(e) => warn!("échec de réindexation {} : {e:#}", path.display()),
    }
}

pub fn save_settings_if_changed(settings: &AppSettings) -> Result<()> {
    save_settings(settings)
}