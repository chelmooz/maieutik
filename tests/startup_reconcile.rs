//! Réconciliation au démarrage : l'index ne doit pas conserver des entrées
//! pour des fichiers qui ne sont plus là. Un fichier supprimé pendant que
//! Maieutik était arrêté reste sinon cherchable indéfiniment.
//!
//! Règle fail-safe testée ici : on ne purge que sur un constat sûr (absent, ou
//! présent mais plus un fichier ordinaire) ; des métadonnées illisibles
//! conservent l'entrée, une réconciliation ne détruit pas sur une ambiguïté.

use maieutik::config::{AppSettings, FileContent};
use maieutik::core::indexer::{Chunker, HashEmbedder, VectorIndex};
use maieutik::core::scan::{index_all, reconcile_startup};
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
async fn vanished_file_is_purged_at_startup() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    std::fs::write(docs.path().join("a.md"), FOX).unwrap();
    std::fs::write(docs.path().join("b.md"), RUST).unwrap();
    let index = open(data.path());
    let settings = settings_for(docs.path());
    index_all(&index, &settings, false).await.unwrap();
    assert_eq!(index.stored_file_paths().unwrap().len(), 2);

    // Suppression pendant que le programme est arrêté : aucun événement.
    std::fs::remove_file(docs.path().join("b.md")).unwrap();

    let report = reconcile_startup(&index).await.unwrap();
    assert_eq!(report.removed.len(), 1, "bilan inattendu : {report:?}");
    assert!(report.removed[0].ends_with("b.md"), "{report:?}");

    let stored = index.stored_file_paths().unwrap();
    assert_eq!(stored.len(), 1, "restant : {stored:?}");
    assert!(stored[0].ends_with("a.md"), "restant : {stored:?}");

    // Le fichier disparu ne doit plus remonter dans la recherche…
    let hits = index.search("rust cargo compiler", 5).await.unwrap();
    assert!(
        hits.iter().all(|(p, _, _)| !p.ends_with("b.md")),
        "le fichier supprimé est encore cherchable : {hits:?}"
    );
    // …et le fichier présent doit le rester.
    let hits = index.search("quick brown fox", 5).await.unwrap();
    assert!(
        hits.iter().any(|(p, _, _)| p.ends_with("a.md")),
        "fichier présent perdu par la réconciliation : {hits:?}"
    );
}

#[tokio::test]
async fn path_replaced_by_directory_is_purged() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    let note = docs.path().join("a.md");
    std::fs::write(&note, FOX).unwrap();
    let index = open(data.path());
    index_all(&index, &settings_for(docs.path()), false).await.unwrap();

    // Le chemin existe encore, mais ce n'est plus un fichier ordinaire.
    std::fs::remove_file(&note).unwrap();
    std::fs::create_dir(&note).unwrap();

    let report = reconcile_startup(&index).await.unwrap();
    assert_eq!(report.removed.len(), 1, "bilan inattendu : {report:?}");
    assert!(index.stored_file_paths().unwrap().is_empty());
}

#[tokio::test]
async fn present_files_are_kept() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    std::fs::write(docs.path().join("a.md"), FOX).unwrap();
    let index = open(data.path());
    index_all(&index, &settings_for(docs.path()), false).await.unwrap();

    let report = reconcile_startup(&index).await.unwrap();
    assert!(report.removed.is_empty(), "bilan inattendu : {report:?}");
    assert!(report.kept_unreadable.is_empty(), "bilan inattendu : {report:?}");
    assert_eq!(index.stored_file_paths().unwrap().len(), 1);
    let hits = index.search("quick brown fox", 5).await.unwrap();
    assert!(hits.iter().any(|(p, _, _)| p.ends_with("a.md")), "{hits:?}");
}

#[tokio::test]
async fn unreadable_metadata_keeps_entry() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    let file = docs.path().join("a.md");
    std::fs::write(&file, "contenu").unwrap();
    let index = open(data.path());

    // Chemin impossible : un fichier ordinaire utilisé comme dossier parent.
    // `metadata` échoue autrement qu'en « introuvable » : l'entrée doit être
    // conservée, pas purgée sur une ambiguïté.
    let ghost = file.join("note.md");
    let base = FileContent {
        path: ghost.clone(),
        headings: Vec::new(),
        text: "contenu fantome".to_string(),
        chunks: Vec::new(),
    };
    let chunker = Chunker::new(1200, 150);
    let chunks = chunker.chunk(&base);
    index
        .store_file(&ghost, &FileContent { chunks, ..base }, None)
        .unwrap();
    assert_eq!(index.stored_file_paths().unwrap().len(), 1);

    let report = reconcile_startup(&index).await.unwrap();
    assert!(report.removed.is_empty(), "bilan inattendu : {report:?}");
    assert_eq!(
        report.kept_unreadable.len(),
        1,
        "l'entrée ambiguë doit être conservée et signalée : {report:?}"
    );
}

/// Le temps que deux écritures successives aient des dates différentes, même
/// sur un système de fichiers à résolution grossière.
fn tick() {
    std::thread::sleep(std::time::Duration::from_millis(20));
}

#[tokio::test]
async fn modified_file_is_reindexed_at_startup() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    std::fs::write(docs.path().join("a.md"), FOX).unwrap();
    let index = open(data.path());
    index_all(&index, &settings_for(docs.path()), false).await.unwrap();
    let hits = index.search("quick brown fox", 5).await.unwrap();
    assert!(hits.iter().any(|(p, _, _)| p.ends_with("a.md")), "{hits:?}");

    // Modification pendant que le programme est arrêté : aucun événement.
    tick();
    std::fs::write(docs.path().join("a.md"), RUST).unwrap();

    let report = reconcile_startup(&index).await.unwrap();
    assert_eq!(report.reindexed.len(), 1, "bilan inattendu : {report:?}");
    assert!(report.reindexed[0].ends_with("a.md"), "{report:?}");

    // Le contenu réindexé est le nouveau : l'ancien ne doit plus remonter…
    let hits = index.search("quick brown fox", 5).await.unwrap();
    assert!(
        hits.iter().all(|(p, _, _)| !p.ends_with("a.md")),
        "l'ancien contenu est encore cherchable : {hits:?}"
    );
    // …et le nouveau doit être trouvé.
    let hits = index.search("rust cargo compiler", 5).await.unwrap();
    assert!(hits.iter().any(|(p, _, _)| p.ends_with("a.md")), "{hits:?}");

    // Idempotence : la réindexation a relevé la nouvelle empreinte, un
    // second démarrage ne doit plus rien réindexer.
    let report = reconcile_startup(&index).await.unwrap();
    assert!(
        report.reindexed.is_empty() && report.kept_unverified.is_empty(),
        "réconciliation non idempotente : {report:?}"
    );
}

#[tokio::test]
async fn rewrite_of_identical_size_is_still_detected() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    std::fs::write(docs.path().join("a.md"), FOX).unwrap();
    let index = open(data.path());
    index_all(&index, &settings_for(docs.path()), false).await.unwrap();

    // Même taille, seul le dernier caractère change : la détection ne peut
    // reposer que sur la date de modification.
    tick();
    let same_size = format!("{}R", &FOX[..FOX.len() - 1]);
    assert_eq!(same_size.len(), FOX.len());
    std::fs::write(docs.path().join("a.md"), same_size).unwrap();

    let report = reconcile_startup(&index).await.unwrap();
    assert_eq!(report.reindexed.len(), 1, "bilan inattendu : {report:?}");
}

#[tokio::test]
async fn unchanged_file_is_not_reindexed() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    std::fs::write(docs.path().join("a.md"), FOX).unwrap();
    let index = open(data.path());
    index_all(&index, &settings_for(docs.path()), false).await.unwrap();

    let report = reconcile_startup(&index).await.unwrap();
    assert!(
        report.reindexed.is_empty()
            && report.removed.is_empty()
            && report.kept_unverified.is_empty()
            && report.kept_unreadable.is_empty()
            && report.failed.is_empty(),
        "bilan inattendu : {report:?}"
    );
}

#[tokio::test]
async fn entry_without_recorded_fingerprint_is_kept_and_reported() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    let file = docs.path().join("a.md");
    std::fs::write(&file, FOX).unwrap();
    let index = open(data.path());

    // Index antérieur au stockage des empreintes : le fichier est présent,
    // son contenu est indexé, mais aucune empreinte n'est enregistrée.
    let base = FileContent {
        path: file.clone(),
        headings: Vec::new(),
        text: FOX.to_string(),
        chunks: Vec::new(),
    };
    let chunks = Chunker::new(1200, 150).chunk(&base);
    index
        .store_file(&file, &FileContent { chunks, ..base }, None)
        .unwrap();

    let report = reconcile_startup(&index).await.unwrap();
    assert!(report.removed.is_empty(), "bilan inattendu : {report:?}");
    assert!(
        report.reindexed.is_empty(),
        "sans empreinte enregistrée, réindexer serait détruire sur une conjecture : {report:?}"
    );
    assert_eq!(
        report.kept_unverified.len(),
        1,
        "l'entrée doit être conservée et signalée : {report:?}"
    );
    let hits = index.search("quick brown fox", 5).await.unwrap();
    assert!(hits.iter().any(|(p, _, _)| p.ends_with("a.md")), "{hits:?}");
}
