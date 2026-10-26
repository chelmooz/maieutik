//! Parcours des dossiers configurés et indexation par lot.

use crate::config::{AppSettings, FileContent};
use crate::core::indexer::{extract_headings, Chunker, VectorIndex};
use crate::core::watch::{is_extension_allowed, is_within_size_limit};
use crate::utils::extract_text;
use anyhow::Result;
use std::path::{Path, PathBuf};
use tracing::{info, warn};
use walkdir::WalkDir;

/// Taille de chunk et recouvrement, en caractères (à calibrer à l'évaluation FR).
pub const CHUNK_MAX_CHARS: usize = 1200;
pub const CHUNK_OVERLAP_CHARS: usize = 150;

/// Bilan d'une indexation.
#[derive(Debug, Default)]
pub struct IndexReport {
    /// Fichiers indexés avec au moins un chunk.
    pub indexed: usize,
    /// Fichiers lus sans produire aucun chunk (purgés de l'index).
    pub no_text: Vec<PathBuf>,
    /// Erreurs de parcours ou d'indexation, par chemin.
    pub failed: Vec<(PathBuf, String)>,
    /// Nombre de dossiers configurés.
    pub roots_total: usize,
    /// Nombre de dossiers configurés réellement lisibles.
    pub roots_ok: usize,
    /// Nombre de fichiers retenus par le parcours (extension et taille valides).
    pub scanned: usize,
}

impl IndexReport {
    /// Dossiers lisibles, mais aucun fichier à indexer.
    pub fn is_empty_corpus(&self) -> bool {
        self.scanned == 0 && self.roots_ok > 0
    }

    /// 4 = aucun dossier configuré n'est lisible ;
    /// 0 = tout est propre ; 1 = échec partiel (échec, sans texte, corpus vide).
    pub fn exit_code(&self) -> i32 {
        if self.roots_total > 0 && self.roots_ok == 0 {
            4
        } else if self.failed.is_empty() && self.no_text.is_empty() && !self.is_empty_corpus() {
            0
        } else {
            1
        }
    }
}

/// Fichiers à indexer : dossiers x extensions autorisées, sans suivre les liens
/// symboliques, taille limitée. Les chemins sont canonicalisés. Les erreurs de
/// parcours sont renvoyées à part, ainsi que le nombre de dossiers lisibles.
pub fn collect_files(settings: &AppSettings) -> (Vec<PathBuf>, Vec<(PathBuf, String)>, usize) {
    let mut files = Vec::new();
    let mut errors = Vec::new();
    let mut roots_ok = 0usize;
    for folder in &settings.folders {
        let configured = PathBuf::from(folder);
        let root = match configured.canonicalize() {
            Ok(r) if r.is_dir() => r,
            Ok(_) => {
                errors.push((configured, "n'est pas un dossier".to_string()));
                continue;
            }
            Err(e) => {
                errors.push((configured, e.to_string()));
                continue;
            }
        };
        roots_ok += 1;
        for entry in WalkDir::new(&root).follow_links(false) {
            match entry {
                Ok(e) => {
                    let p = e.path();
                    if e.file_type().is_file() && is_extension_allowed(p, settings) {
                        match is_within_size_limit(p, settings) {
                            Ok(true) => files.push(p.to_path_buf()),
                            // Trop gros : ignoré, comme avant.
                            Ok(false) => {}
                            // Métadonnées illisibles : fail-closed, compté
                            // comme échec plutôt qu'accepté en silence.
                            Err(err) => {
                                errors.push((p.to_path_buf(), format!("taille illisible : {err}")))
                            }
                        }
                    }
                }
                Err(err) => {
                    let path = err
                        .path()
                        .map(Path::to_path_buf)
                        .unwrap_or_else(|| root.clone());
                    errors.push((path, err.to_string()));
                }
            }
        }
    }
    files.sort();
    files.dedup();
    (files, errors, roots_ok)
}

/// Indexe tous les fichiers trouvés. `force` purge d'abord l'index (retire aussi
/// les fichiers disparus) ; sinon chaque fichier trouvé est remplacé sur place.
/// Une erreur sur un fichier est journalisée et comptée, jamais fatale, et ne
/// purge pas l'ancien contenu du fichier. HNSW est reconstruit une seule fois.
/// Si aucun dossier n'est lisible, on s'arrête avant toute purge : un `force`
/// ne doit jamais vider l'index à cause d'un disque ou d'un dossier absent.
pub async fn index_all(
    index: &VectorIndex,
    settings: &AppSettings,
    force: bool,
) -> Result<IndexReport> {
    let (files, walk_errors, roots_ok) = collect_files(settings);
    let mut report = IndexReport {
        roots_total: settings.folders.len(),
        roots_ok,
        scanned: files.len(),
        ..IndexReport::default()
    };
    for (path, msg) in walk_errors {
        warn!("scan error {}: {}", path.display(), msg);
        report.failed.push((path, msg));
    }
    if roots_ok == 0 {
        return Ok(report);
    }
    if force {
        index.clear().await?;
    }
    let chunker = Chunker::new(CHUNK_MAX_CHARS, CHUNK_OVERLAP_CHARS);
    for path in files {
        match index_one(index, &chunker, &path) {
            Ok(true) => report.indexed += 1,
            Ok(false) => {
                warn!("no text {}", path.display());
                report.no_text.push(path);
            }
            Err(e) => {
                warn!("index error {}: {:#}", path.display(), e);
                report.failed.push((path, format!("{e:#}")));
            }
        }
    }
    index.build_hnsw().await?;
    info!(
        "indexed {} file(s), {} without text, {} failure(s)",
        report.indexed,
        report.no_text.len(),
        report.failed.len()
    );
    Ok(report)
}

/// Renvoie `true` si le fichier a produit au moins un chunk. Un fichier sans
/// texte est quand même transmis à l'index (liste de chunks vide) afin de
/// purger ses anciens enregistrements.
fn index_one(index: &VectorIndex, chunker: &Chunker, path: &Path) -> Result<bool> {
    let text = extract_text(path)?;
    let base = FileContent {
        path: path.to_path_buf(),
        headings: extract_headings(&text),
        text,
        chunks: Vec::new(),
    };
    let chunks = chunker.chunk(&base);
    let has_text = !chunks.is_empty();
    index.store_file(path, &FileContent { chunks, ..base })?;
    Ok(has_text)
}

/// Indexe un fichier isolé puis reconstruit HNSW (chemin « watcher » : un
/// événement à la fois, contrairement à `index_all` qui reconstruit une fois
/// par lot). `Ok(true)` = au moins un chunk ; `Ok(false)` = fichier lu sans
/// texte, dont les anciens enregistrements ont été purgés. L'appelant valide
/// l'existence, l'extension et la taille avant l'appel.
pub async fn index_path(index: &VectorIndex, path: &Path) -> Result<bool> {
    let chunker = Chunker::new(CHUNK_MAX_CHARS, CHUNK_OVERLAP_CHARS);
    let has_text = index_one(index, &chunker, path)?;
    index.build_hnsw().await?;
    Ok(has_text)
}
