//! `index --force` ne doit jamais détruire des données encore valides.
//!
//! Un `--force` qui purge d'abord l'index entier perd l'ancien contenu d'un
//! fichier qui échoue à l'extraction au milieu du lot (fichier devenu
//! illisible pendant le scan). Le contrat testé ici : le force remplace les
//! fichiers présents, purge les fichiers disparus, et préserve l'ancien
//! contenu d'un fichier en échec — comme le mode normal.

use maieutik::config::AppSettings;
use maieutik::core::indexer::{HashEmbedder, VectorIndex};
use maieutik::core::scan::index_all;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

const FOX: &str = "the quick brown fox jumps over the lazy dog near the river";
const RUST: &str = "rust cargo compiler borrow checker lifetimes traits";

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

#[tokio::test]
async fn force_keeps_records_when_extraction_fails_midway() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    std::fs::write(docs.path().join("a.md"), FOX).unwrap();
    std::fs::write(docs.path().join("b.md"), RUST).unwrap();
    let index = open(data.path());
    let settings = settings_for(docs.path());
    index_all(&index, &settings, false).await.unwrap();
    assert_eq!(index.stats().0, 2);

    // a.md devient illisible (UTF-8 invalide) : l'extraction échoue.
    std::fs::write(docs.path().join("a.md"), [0xff, 0xfe, 0x00, 0xda]).unwrap();

    let report = index_all(&index, &settings, true).await.unwrap();
    assert!(
        report.failed.iter().any(|(p, _)| p.ends_with("a.md")),
        "l'échec d'extraction doit être compté : {:?}",
        report.failed
    );

    // L'ancien contenu de a.md doit rester cherchable : un --force ne purge
    // pas des données encore valides quand un fichier devient illisible.
    let hits = index.search("quick brown fox", 5).await.unwrap();
    assert!(
        hits.iter().any(|(p, _, _)| p.ends_with("a.md")),
        "ancien contenu de a.md perdu pendant le force : {hits:?}"
    );
    // b.md, lisible, doit être réindexé.
    let hits = index.search("rust cargo compiler", 5).await.unwrap();
    assert!(
        hits.iter().any(|(p, _, _)| p.ends_with("b.md")),
        "b.md doit être réindexé : {hits:?}"
    );
}

#[tokio::test]
async fn force_purges_files_that_disappeared_between_runs() {
    // Non-régression : le force retire toujours les fichiers disparus,
    // mais jamais ceux encore présents.
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    std::fs::write(docs.path().join("a.md"), FOX).unwrap();
    std::fs::write(docs.path().join("b.md"), RUST).unwrap();
    let index = open(data.path());
    let settings = settings_for(docs.path());
    index_all(&index, &settings, false).await.unwrap();

    std::fs::remove_file(docs.path().join("b.md")).unwrap();
    index_all(&index, &settings, true).await.unwrap();

    let hits = index.search("rust cargo compiler", 5).await.unwrap();
    assert!(
        !hits.iter().any(|(p, _, _)| p.ends_with("b.md")),
        "b.md disparu doit être purgé de l'index : {hits:?}"
    );
    let hits = index.search("quick brown fox", 5).await.unwrap();
    assert!(
        hits.iter().any(|(p, _, _)| p.ends_with("a.md")),
        "a.md encore présent doit rester cherchable : {hits:?}"
    );
}
