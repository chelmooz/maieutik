//! Indexation et recherche sans modèle ONNX : faux embedder déterministe.

use maieutik::config::{AppSettings, FileContent};
use maieutik::core::indexer::{
    extract_headings, Chunker, HashEmbedder, HnswIndex, TextEmbedder, VectorIndex,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::RwLock;

const FOX: &str = "the quick brown fox jumps over the lazy dog near the river";
const RUST: &str = "rust cargo compiler borrow checker lifetimes traits";
const BREAD: &str = "bread flour yeast oven baking sourdough starter";

fn open(dir: &Path) -> VectorIndex {
    let settings = Arc::new(RwLock::new(AppSettings::default()));
    VectorIndex::open(dir, settings, HashEmbedder::new()).unwrap()
}

fn content(path: &str, text: &str, max_chars: usize) -> FileContent {
    let base = FileContent {
        path: PathBuf::from(path),
        text: text.to_string(),
        headings: extract_headings(text),
        chunks: Vec::new(),
    };
    let chunks = Chunker::new(max_chars, 0).chunk(&base);
    FileContent { chunks, ..base }
}

#[test]
fn hash_embedder_is_deterministic_and_normalized() {
    let e = HashEmbedder::new();
    let a = e.embed("Rust borrow checker").unwrap();
    let b = e.embed("rust BORROW checker").unwrap();
    assert_eq!(a, b);
    assert_eq!(a.len(), 1024);
    let norm: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!((norm - 1.0).abs() < 1e-5);
    assert_ne!(a, e.embed("sourdough starter").unwrap());
}

#[tokio::test]
async fn search_returns_the_matching_file_first() {
    let dir = tempfile::tempdir().unwrap();
    let index = open(dir.path());
    for (name, text) in [("fox.md", FOX), ("rust.md", RUST), ("bread.md", BREAD)] {
        index
            .upsert_file(Path::new(name), &content(name, text, 1000))
            .await
            .unwrap();
    }
    let hits = index.search("rust borrow checker", 3).await.unwrap();
    assert_eq!(hits[0].0, "rust.md");
}

#[tokio::test]
async fn upsert_replaces_previous_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let index = open(dir.path());
    let long = format!("{FOX} {RUST} {BREAD}");
    index
        .upsert_file(Path::new("a.md"), &content("a.md", &long, 60))
        .await
        .unwrap();
    assert!(index.stats().0 > 1);
    index
        .upsert_file(Path::new("a.md"), &content("a.md", "short", 1000))
        .await
        .unwrap();
    assert_eq!(index.stats().0, 1);
}

#[tokio::test]
async fn upsert_with_no_chunks_purges_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let index = open(dir.path());
    index
        .upsert_file(Path::new("a.md"), &content("a.md", FOX, 1000))
        .await
        .unwrap();
    assert_eq!(index.stats().0, 1);
    index
        .upsert_file(Path::new("a.md"), &content("a.md", "", 1000))
        .await
        .unwrap();
    assert_eq!(index.stats().0, 0);
}
/// `is_empty` est le pendant de `len` : un index neuf n'a aucun vecteur, et le
/// premier ajout le rend non vide. Les deux lectures doivent rester d'accord.
#[test]
fn hnsw_is_empty_until_the_first_vector() {
    let mut index = HnswIndex::new(3);
    assert!(index.is_empty(), "un index neuf ne contient aucun vecteur");
    assert_eq!(index.len(), 0);

    index.add("chunk-0".to_string(), &[1.0, 0.0, 0.0]).unwrap();
    assert!(!index.is_empty(), "un vecteur ajoute rend l'index non vide");
    assert_eq!(index.len(), 1);
}
