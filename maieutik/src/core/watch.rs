//! File watcher with debouncing.

use crate::config::AppSettings;
use anyhow::Result;
use notify::event::ModifyKind;
use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{debug, info, warn};

pub struct FileWatcher {
    watcher: RecommendedWatcher,
    settings: Arc<tokio::sync::RwLock<AppSettings>>,
    /// Gardé vivant pour le canal : si ce sender tombait, le `Receiver` du
    /// service verrait la fin du canal et s'arrêterait sans erreur.
    #[allow(dead_code)]
    tx: mpsc::Sender<WatchEvent>,
}

#[derive(Debug, Clone)]
pub enum WatchEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Removed(PathBuf),
    Renamed { from: PathBuf, to: PathBuf },
}

/// Racines réellement surveillables : chemins canonicalisés (mêmes clés
/// d'index que `scan::collect_files`), relatifs résolus contre `base`.
/// Les dossiers invalides sont renvoyés à part avec leur motif, jamais
/// ignorés en silence.
pub fn resolve_watch_roots(
    folders: &[String],
    base: &Path,
) -> (Vec<PathBuf>, Vec<(PathBuf, String)>) {
    let mut roots = Vec::new();
    let mut errors = Vec::new();
    for folder in folders {
        let configured = PathBuf::from(folder);
        let candidate = if configured.is_absolute() {
            configured.clone()
        } else {
            base.join(&configured)
        };
        match candidate.canonicalize() {
            Ok(root) if root.is_dir() => roots.push(root),
            Ok(_) => errors.push((configured, "n'est pas un dossier".to_string())),
            Err(e) => errors.push((configured, e.to_string())),
        }
    }
    roots.sort();
    roots.dedup();
    (roots, errors)
}

impl FileWatcher {
    pub fn new(
        settings: Arc<tokio::sync::RwLock<AppSettings>>,
        tx: mpsc::Sender<WatchEvent>,
    ) -> Result<Self> {
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
        let cwd = std::env::current_dir()?;
        let (roots, errors) = resolve_watch_roots(&folders, &cwd);
        for (path, reason) in &errors {
            warn!("Dossier à surveiller ignoré {} : {reason}", path.display());
        }
        if roots.is_empty() {
            // Liste vide : rien à surveiller, ce n'est pas une erreur (le
            // binaire refuse déjà ce cas en amont). Des dossiers configurés
            // mais tous invalides : un watcher tournerait à vide en silence,
            // mieux vaut refuser avec la liste des chemins fautifs.
            if folders.is_empty() {
                info!("no folder configured, nothing to watch");
                return Ok(());
            }
            let listed = errors
                .iter()
                .map(|(path, reason)| format!("{} ({reason})", path.display()))
                .collect::<Vec<_>>()
                .join(", ");
            anyhow::bail!("aucun dossier surveillable parmi {listed}");
        }
        for root in roots {
            self.watcher.watch(&root, RecursiveMode::Recursive)?;
            info!("Watching folder: {}", root.display());
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
                event = self.rx.recv() => match event {
                    Some(event) => self.on_event(event).await,
                    None => {
                        // Tous les senders sont tombés : on vide ce qui reste
                        // puis on rend la main, sinon la boucle survivrait
                        // indéfiniment à son canal en ne faisant que tiquer.
                        debug!("canal d'événements fermé : vidage puis arrêt du débounce");
                        self.flush_all().await;
                        break;
                    }
                },
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
        let ready: Vec<_> = self
            .pending
            .iter()
            .filter(|(_, (_, ts))| now.duration_since(*ts) >= self.debounce)
            .map(|(p, _)| p.clone())
            .collect();
        for path in ready {
            if let Some((event, _)) = self.pending.remove(&path) {
                self.emit(event).await;
            }
        }
    }

    /// Émet sans attendre la fenêtre de débounce tout ce qui est en attente,
    /// dans l'ordre d'arrivée. Appelé quand le canal se ferme : les derniers
    /// changements ne doivent pas être perdus à l'arrêt.
    async fn flush_all(&mut self) {
        let pending = std::mem::take(&mut self.pending);
        let mut events: Vec<(WatchEvent, tokio::time::Instant)> = pending.into_values().collect();
        events.sort_by_key(|(_, ts)| *ts);
        for (event, _) in events {
            self.emit(event).await;
        }
    }

    async fn emit(&self, event: WatchEvent) {
        if let Err(e) = self.process_tx.send(event).await {
            warn!("Failed to send debounced event: {}", e);
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
        settings
            .extensions
            .iter()
            .any(|e| e.eq_ignore_ascii_case(&ext))
    } else {
        false
    }
}

/// Limite de taille en octets. `max_file_size` est exprimé en mébioctets :
/// la conversion sature à `u64::MAX` au lieu de déborder — sans cela, un
/// réglage démesuré panique en build debug (« attempt to multiply with
/// overflow ») et produit une valeur fausse en release.
#[allow(clippy::manual_saturating_arithmetic)] // `checked_mul` explicite : la saturation à u64::MAX est voulue.
pub fn max_file_size_bytes(settings: &AppSettings) -> u64 {
    (settings.max_file_size as u64)
        .checked_mul(1024 * 1024)
        .unwrap_or(u64::MAX)
}

/// Prédicat pur : une taille connue tient-elle dans la limite configurée ?
pub fn is_indexable_size(size: u64, settings: &AppSettings) -> bool {
    size <= max_file_size_bytes(settings)
}

/// Éligibilité d'un fichier au corpus : extension autorisée ET taille sous la
/// limite. Point de vérité unique, partagé par le parcours initial
/// (`scan::collect_files`), la réconciliation de démarrage et le watcher.
/// `Err` = métadonnées illisibles (fail-closed) : jamais une acceptation
/// implicite.
pub fn is_indexable_file(path: &Path, settings: &AppSettings) -> Result<bool> {
    if !is_extension_allowed(path, settings) {
        return Ok(false);
    }
    is_within_size_limit(path, settings)
}

/// Contrôle de la limite de taille (Mo dans les réglages). Fail-closed : un
/// fichier dont les métadonnées sont illisibles est une erreur, jamais une
/// autorisation implicite — l'appelant rejette et diagnostique.
pub fn is_within_size_limit(path: &Path, settings: &AppSettings) -> Result<bool> {
    let meta = std::fs::metadata(path)?;
    Ok(is_indexable_size(meta.len(), settings))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings(max_mb: usize) -> AppSettings {
        AppSettings {
            max_file_size: max_mb,
            ..AppSettings::default()
        }
    }

    #[test]
    fn size_limit_fails_closed_on_unreadable_path() {
        let missing = PathBuf::from("/chemin/absent/garanti/note.md");
        assert!(is_within_size_limit(&missing, &settings(10)).is_err());
    }

    #[test]
    fn size_limit_accepts_file_within_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.md");
        std::fs::write(&file, "hello").unwrap();
        assert!(is_within_size_limit(&file, &settings(10)).unwrap());
    }

    #[test]
    fn size_limit_rejects_file_above_limit() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("a.md");
        std::fs::write(&file, "hello").unwrap();
        assert!(!is_within_size_limit(&file, &settings(0)).unwrap());
    }

    #[tokio::test]
    async fn watch_rejects_all_missing_roots_and_canonicalizes_relative_paths() {
        let dir = tempfile::tempdir().unwrap();
        let notes = dir.path().join("notes");
        std::fs::create_dir(&notes).unwrap();
        let missing = dir.path().join("absent");

        // Un chemin relatif doit être résolu contre `base` puis canonicalisé,
        // comme `scan::collect_files` : mêmes clés d'index des deux côtés.
        let folders = vec!["notes".to_string(), missing.to_string_lossy().to_string()];
        let (roots, errors) = resolve_watch_roots(&folders, dir.path());
        assert_eq!(
            roots,
            vec![notes.canonicalize().unwrap()],
            "racine relative non canonicalisée : {roots:?}"
        );
        assert_eq!(errors.len(), 1, "dossier absent non signalé : {errors:?}");
        assert_eq!(errors[0].0, missing);

        // Aucune racine valide : le watcher doit refuser de démarrer au lieu
        // de tourner à vide en silence.
        let dead = AppSettings {
            folders: vec!["absent-a".to_string(), "absent-b".to_string()],
            ..AppSettings::default()
        };
        let (tx, _rx) = mpsc::channel(4);
        let mut watcher = FileWatcher::new(Arc::new(tokio::sync::RwLock::new(dead)), tx).unwrap();
        let err = watcher.watch().await.unwrap_err();
        let message = format!("{err:#}");
        assert!(
            message.contains("absent-a") && message.contains("absent-b"),
            "le message doit nommer les racines fautives : {message}"
        );
    }
}
