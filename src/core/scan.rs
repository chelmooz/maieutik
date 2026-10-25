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
    pub indexed: usize,
    pub failed: Vec<(PathBuf, String)>,
}

impl IndexReport {
    /// 0 = tout est indexé, 1 = au moins un échec.
    pub fn exit_code(&self) -> i32 {
        if self.failed.is_empty() {
            0
        } else {
            1
        }
    }
}

/// Fichiers à indexer : dossiers x extensions autorisées, sans suivre les liens
/// symboliques, taille limitée. Les chemins sont canonicalisés. Les erreurs de
/// parcours sont renvoyées à part.
pub fn collect_files(settings: &AppSettings) -> (Vec<PathBuf>, Vec<(PathBuf, String)>) {
    let mut files = Vec::new();
    let mut errors = Vec::new();
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
        for entry in WalkDir::new(&root).follow_links(false) {
            match entry {
                Ok(e) => {
                    let p = e.path();
                    if e.file_type().is_file()
                        && is_extension_allowed(p, settings)
                        && is_within_size_limit(p, settings)
                    {
                        files.push(p.to_path_buf());
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
    (files, errors)
}

/// Indexe tous les fichiers trouvés. `force` purge d'abord l'index (retire aussi
/// les fichiers disparus) ; sinon chaque fichier trouvé est remplacé sur place.
/// Une erreur sur un fichier est journalisée et comptée, jamais fatale, et ne
/// purge pas l'ancien contenu du fichier. HNSW est reconstruit une seule fois.
pub async fn index_all(
    index: &VectorIndex,
    settings: &AppSettings,
    force: bool,
) -> Result<IndexReport> {
    let (files, walk_errors) = collect_files(settings);
    let mut report = IndexReport::default();
    for (path, msg) in walk_errors {
        warn!("scan error {}: {}", path.display(), msg);
        report.failed.push((path, msg));
    }
    if force {
        index.clear().await?;
    }
    let chunker = Chunker::new(CHUNK_MAX_CHARS, CHUNK_OVERLAP_CHARS);
    for path in files {
        match index_one(index, &chunker, &path) {
            Ok(()) => report.indexed += 1,
            Err(e) => {
                warn!("index error {}: {:#}", path.display(), e);
                report.failed.push((path, format!("{e:#}")));
            }
        }
    }
    index.build_hnsw().await?;
    info!(
        "indexed {} file(s), {} failure(s)",
        report.indexed,
        report.failed.len()
    );
    Ok(report)
}

fn index_one(index: &VectorIndex, chunker: &Chunker, path: &Path) -> Result<()> {
    let text = extract_text(path)?;
    let base = FileContent {
        path: path.to_path_buf(),
        headings: extract_headings(&text),
        text,
        chunks: Vec::new(),
    };
    let chunks = chunker.chunk(&base);
    index.store_file(path, &FileContent { chunks, ..base })
}