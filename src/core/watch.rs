//! File watcher with debouncing.

use anyhow::Result;
use crate::config::{FileItem, AppSettings};
use notify::event::ModifyKind;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::sleep;
use tracing::{debug, info, warn};

pub struct FileWatcher {
    watcher: RecommendedWatcher,
    settings: Arc<tokio::sync::RwLock<AppSettings>>,
    tx: mpsc::Sender<WatchEvent>,
}

#[derive(Debug, Clone)]
pub enum WatchEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Removed(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}

impl FileWatcher {
    pub fn new(settings: Arc<tokio::sync::RwLock<AppSettings>>, tx: mpsc::Sender<WatchEvent>) -> Result<Self> {
        let tx_for_events = tx.clone();
        let watcher = notify::recommended_watcher(move |res: Result<Event, notify::Error>| {
            let event = match res {
                Ok(event) => event,
                Err(err) => {
                    warn!("watch error: {err}");
                    return;
                }
            };
            let paths = event.paths;
            let watch_event = match event.kind {
                EventKind::Create(_) => paths.first().cloned().map(WatchEvent::Created),
                EventKind::Remove(_) => paths.first().cloned().map(WatchEvent::Removed),
                EventKind::Modify(ModifyKind::Name(_)) => {
                    if paths.len() >= 2 {
                        Some(WatchEvent::Renamed {
                            from: paths[0].clone(),
                            to: paths[1].clone(),
                        })
                    } else {
                        paths.first().cloned().map(WatchEvent::Modified)
                    }
                }
                EventKind::Modify(_) => paths.first().cloned().map(WatchEvent::Modified),
                _ => None,
            };
            if let Some(watch_event) = watch_event {
                let _ = tx_for_events.blocking_send(watch_event);
            }
        })?;

        Ok(Self {
            watcher,
            settings,
            tx,
        })
    }

    pub async fn watch(&mut self) -> Result<()> {
        let folders = self.settings.read().await.folders.clone();
        for folder in folders {
            let path = PathBuf::from(&folder);
            if path.exists() {
                self.watcher.watch(&path, RecursiveMode::Recursive)?;
                info!("Watching folder: {}", path.display());
            } else {
                warn!("Watch folder does not exist: {}", path.display());
            }
        }
        Ok(())
    }

    pub fn unwatch_all(&mut self) {
        // notify 6.x: unwatch_all not available; watcher will be dropped naturally
    }
}

/// Debounced event processor (2 s default window).
pub struct DebouncedProcessor {
    rx: mpsc::Receiver<WatchEvent>,
    pending: std::collections::HashMap<PathBuf, (WatchEvent, tokio::time::Instant)>,
    debounce: Duration,
    process_tx: mpsc::Sender<WatchEvent>,
}

impl DebouncedProcessor {
    pub fn new(rx: mpsc::Receiver<WatchEvent>, process_tx: mpsc::Sender<WatchEvent>) -> Self {
        Self {
            rx,
            pending: HashMap::new(),
            debounce: Duration::from_secs(2),
            process_tx,
        }
    }

    pub async fn run(mut self) {
        let mut interval = tokio::time::interval(Duration::from_millis(250));
        loop {
            tokio::select! {
                Some(event) = self.rx.recv() => {
                    self.on_event(event).await;
                }
                _ = interval.tick() => {
                    self.flush_ready().await;
                }
            }
        }
    }

    async fn on_event(&mut self, event: WatchEvent) {
        let path = match &event {
            WatchEvent::Created(p) | WatchEvent::Modified(p) | WatchEvent::Removed(p) => p.clone(),
            WatchEvent::Renamed { to, .. } => to.clone(),
        };
        let now = tokio::time::Instant::now();
        self.pending.insert(path, (event, now));
    }

    async fn flush_ready(&mut self) {
        let now = tokio::time::Instant::now();
        let ready: Vec<_> = self.pending
            .iter()
            .filter(|(_, (_, ts))| now.duration_since(*ts) >= self.debounce)
            .map(|(p, _)| p.clone())
            .collect();
        for path in ready {
            if let Some((event, _)) = self.pending.remove(&path) {
                if let Err(e) = self.process_tx.send(event).await {
                    warn!("Failed to send debounced event: {}", e);
                }
            }
        }
    }
}

/// Check if a path is inside any watched folder.
pub fn is_path_watched(path: &Path, settings: &AppSettings) -> bool {
    let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    for folder in &settings.folders {
        let folder_path = PathBuf::from(folder);
        if let Ok(canon_folder) = folder_path.canonicalize() {
            if canonical.starts_with(&canon_folder) {
                return true;
            }
        }
    }
    false
}

/// Check if the file extension is allowed.
pub fn is_extension_allowed(path: &Path, settings: &AppSettings) -> bool {
    if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
        let ext = format!(".{}", ext.to_lowercase());
        settings.extensions.iter().any(|e| e.eq_ignore_ascii_case(&ext))
    } else {
        false
    }
}

/// Check the file size limit.
pub fn is_within_size_limit(path: &Path, settings: &AppSettings) -> bool {
    if let Ok(meta) = std::fs::metadata(path) {
        let max_bytes = (settings.max_file_size as u64) * 1024 * 1024;
        meta.len() <= max_bytes
    } else {
        true // if can't read, allow
    }
}