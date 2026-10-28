//! Réindexation d'un fichier isolé (chemin « watcher ») sans modèle ONNX.
//!
//! Le watcher traite chaque événement par la séquence validée ici :
//! `index_path` pour un fichier créé/modifié/renommé (cible), `remove_file`
//! pour la source d'un renommage. Chaque test verrouille le comportement de
//! ce seam, indépendamment de `notify` (débounce, latence FS).

use maieutik::core::indexer::{HashEmbedder, VectorIndex};
use maieutik::core::scan::index_path;
use std::path::Path;

const FOX: &str = "the quick brown fox jumps over the lazy dog near the river";
const RUST: &str = "rust cargo compiler borrow checker lifetimes traits";

fn open(dir: &Path) -> VectorIndex {
    VectorIndex::open(dir, HashEmbedder::new()).unwrap()
}

async fn hits_for(idx: &VectorIndex, query: &str) -> Vec<(String, String)> {
    idx.search(query, 5)
        .await
        .unwrap()
        .into_iter()
        .map(|(p, t, _)| (p, t))
        .collect()
}

#[tokio::test]
async fn created_file_is_indexed_and_found() {
    let dir = tempfile::tempdir().unwrap();
    let idx = open(dir.path());
    let note = dir.path().join("a.md");
    std::fs::write(&note, FOX).unwrap();

    assert!(
        index_path(&idx, &note).await.unwrap(),
        "un fichier texte doit produire des chunks"
    );

    let hits = hits_for(&idx, "quick brown fox").await;
    assert!(
        hits.iter().any(|(p, _)| p.ends_with("a.md")),
        "recherche : {hits:?}"
    );
}

#[tokio::test]
async fn modified_file_reflects_new_content_and_purges_old() {
    let dir = tempfile::tempdir().unwrap();
    let idx = open(dir.path());
    let note = dir.path().join("a.md");
    std::fs::write(&note, FOX).unwrap();
    assert!(index_path(&idx, &note).await.unwrap());

    std::fs::write(&note, RUST).unwrap();
    assert!(index_path(&idx, &note).await.unwrap());

    let new_hits = hits_for(&idx, "rust cargo compiler").await;
    assert!(
        new_hits
            .iter()
            .any(|(p, t)| p.ends_with("a.md") && t.contains("rust cargo")),
        "nouveau contenu attendu : {new_hits:?}"
    );
    let old_hits = hits_for(&idx, "quick brown fox").await;
    assert!(
        old_hits
            .iter()
            .all(|(p, t)| !(p.ends_with("a.md") && t.contains("quick brown fox"))),
        "ancien contenu doit être purgé : {old_hits:?}"
    );
}

#[tokio::test]
async fn emptied_file_is_purged_and_reports_no_text() {
    let dir = tempfile::tempdir().unwrap();
    let idx = open(dir.path());
    let note = dir.path().join("a.md");
    std::fs::write(&note, FOX).unwrap();
    assert!(index_path(&idx, &note).await.unwrap());
    assert_eq!(idx.stats().0, 1, "un chunk attendu après indexation");

    std::fs::write(&note, "").unwrap();
    assert!(
        !index_path(&idx, &note).await.unwrap(),
        "un fichier vide ne produit aucun chunk"
    );
    assert_eq!(idx.stats().0, 0, "les enregistrements doivent être purgés");
}

#[tokio::test]
async fn renamed_file_moves_records_from_old_to_new_path() {
    let dir = tempfile::tempdir().unwrap();
    let idx = open(dir.path());
    let from = dir.path().join("a.md");
    let to = dir.path().join("b.md");
    std::fs::write(&from, FOX).unwrap();
    assert!(index_path(&idx, &from).await.unwrap());

    // Séquence du watcher sur renommage : purge de la source, index de la cible.
    std::fs::rename(&from, &to).unwrap();
    idx.remove_file(&from).await.unwrap();
    assert!(index_path(&idx, &to).await.unwrap());

    let hits = hits_for(&idx, "quick brown fox").await;
    assert!(
        hits.iter().any(|(p, _)| p.ends_with("b.md")),
        "la cible du renommage doit être trouvée : {hits:?}"
    );
    assert!(
        !hits.iter().any(|(p, _)| p.ends_with("a.md")),
        "l'ancien chemin doit être purgé : {hits:?}"
    );
}

#[tokio::test]
async fn unreadable_path_propagates_the_error() {
    let dir = tempfile::tempdir().unwrap();
    let idx = open(dir.path());
    let not_a_file = dir.path().join("sous-dossier");
    std::fs::create_dir_all(&not_a_file).unwrap();

    let result = index_path(&idx, &not_a_file).await;
    assert!(result.is_err(), "un dossier n'est pas indexable");
}
