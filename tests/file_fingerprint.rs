//! Empreinte de fichier dans l'index : taille + date de modification,
//! relevées **avant** la lecture du contenu, et restituibles par chemin.
//!
//! C'est ce qui permet à la réconciliation de démarrage (P1.22) de dépasser
//! l'existence du chemin et de détecter un fichier modifié pendant que
//! Maieutik était arrêté — sans changer `VectorRecord`, donc sans purger les
//! index existants : l'empreinte vit dans un arbre sleed séparé.

use maieutik::config::{AppSettings, FileContent};
use maieutik::core::indexer::{Chunker, FileFingerprint, HashEmbedder, VectorIndex};
use maieutik::core::scan::index_all;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

const FOX: &str = "the quick brown fox jumps over the lazy dog near the river";

fn open(dir: &Path) -> VectorIndex {
    let settings = Arc::new(RwLock::new(AppSettings::default()));
    VectorIndex::open(dir, settings, HashEmbedder::new()).unwrap()
}

fn settings_for(folder: &Path) -> AppSettings {
    AppSettings {
        folders: vec![folder.to_string_lossy().to_string()],
        ..AppSettings::default()
    }
}

fn disk_fingerprint(path: &Path) -> FileFingerprint {
    FileFingerprint::read(path)
        .unwrap()
        .expect("empreinte attendue sur un fichier ordinaire")
}

#[tokio::test]
async fn indexing_records_the_disk_fingerprint() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    let file = docs.path().join("a.md");
    std::fs::write(&file, FOX).unwrap();
    let index = open(data.path());
    index_all(&index, &settings_for(docs.path()), false).await.unwrap();

    let stored = index.stored_files().unwrap();
    assert_eq!(stored.len(), 1, "{stored:?}");
    let (path, recorded) = &stored[0];
    assert!(path.ends_with("a.md"), "{stored:?}");
    let recorded = recorded.expect("l'empreinte doit être enregistrée à l'indexation");
    assert_eq!(recorded, disk_fingerprint(&file));
    assert_eq!(recorded.size, FOX.len() as u64);
    assert_eq!(index.fingerprint_of(path).unwrap(), Some(recorded));
}

#[tokio::test]
async fn rewrite_then_reindex_rotates_the_fingerprint() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    let file = docs.path().join("a.md");
    std::fs::write(&file, FOX).unwrap();
    let index = open(data.path());
    index_all(&index, &settings_for(docs.path()), false).await.unwrap();
    let before = index
        .fingerprint_of(&file.to_string_lossy())
        .unwrap()
        .expect("empreinte attendue après indexation");

    std::thread::sleep(std::time::Duration::from_millis(20));
    std::fs::write(&file, format!("{FOX} and more")).unwrap();
    index_all(&index, &settings_for(docs.path()), false).await.unwrap();

    let after = index
        .fingerprint_of(&file.to_string_lossy())
        .unwrap()
        .expect("empreinte attendue après réindexation");
    assert_ne!(before, after, "l'empreinte doit suivre la version indexée");
    assert_eq!(after, disk_fingerprint(&file));
}

#[tokio::test]
async fn purge_drops_the_fingerprint_with_the_records() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    let file = docs.path().join("a.md");
    std::fs::write(&file, FOX).unwrap();
    let index = open(data.path());
    index_all(&index, &settings_for(docs.path()), false).await.unwrap();
    let path = file.to_string_lossy().to_string();
    assert!(index.fingerprint_of(&path).unwrap().is_some());

    index.purge_file(&path).unwrap();

    assert!(index.stored_files().unwrap().is_empty());
    assert_eq!(
        index.fingerprint_of(&path).unwrap(),
        None,
        "purger un fichier doit aussi purger son empreinte, sinon un futur\n\
         réindex du même chemin croirait retrouver une version connue"
    );
}

#[tokio::test]
async fn empty_file_keeps_no_stale_fingerprint_entry() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    let file = docs.path().join("a.md");
    std::fs::write(&file, FOX).unwrap();
    let index = open(data.path());
    index_all(&index, &settings_for(docs.path()), false).await.unwrap();
    let path = file.to_string_lossy().to_string();

    // Un fichier vidé produit zéro chunk : ses enregistrements sont purgés
    // (comportement documenté de store_file). L'empreinte ne doit pas rester.
    std::fs::write(&file, "").unwrap();
    let base = FileContent {
        path: file.clone(),
        headings: Vec::new(),
        text: String::new(),
        chunks: Vec::new(),
    };
    let fingerprint = FileFingerprint::read(&file).unwrap();
    index.store_file(&file, &base, fingerprint).unwrap();

    assert!(index.stored_files().unwrap().is_empty());
    assert_eq!(index.fingerprint_of(&path).unwrap(), None);
}
