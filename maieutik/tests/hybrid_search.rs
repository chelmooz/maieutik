//! Recherche hybride sans modèle ONNX : HashEmbedder + BM25 réel.

use maieutik::config::FileContent;
use maieutik::core::indexer::{extract_headings, Chunker, HashEmbedder, VectorIndex};
use maieutik::core::retrieval::SearchMode;
use std::path::Path;

const FOX: &str = "the quick brown fox jumps over the lazy dog near the river";
const RUST: &str = "rust cargo compiler borrow checker lifetimes traits";
const BREAD: &str = "bread flour yeast oven baking sourdough starter";

fn open(dir: &Path) -> VectorIndex {
    VectorIndex::open(dir, HashEmbedder::new()).unwrap()
}

fn content(path: &str, text: &str, max_chars: usize) -> FileContent {
    let base = FileContent {
        path: std::path::PathBuf::from(path),
        text: text.to_string(),
        headings: extract_headings(text),
        chunks: Vec::new(),
    };
    let chunks = Chunker::new(max_chars, 0).chunk(&base);
    FileContent { chunks, ..base }
}

async fn seed(index: &VectorIndex) {
    for (name, text) in [("fox.md", FOX), ("rust.md", RUST), ("bread.md", BREAD)] {
        index
            .upsert_file(Path::new(name), &content(name, text, 1000))
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn lexical_mode_finds_the_keyword_match() {
    let dir = tempfile::tempdir().unwrap();
    let index = open(dir.path());
    seed(&index).await;

    let hits = index
        .search_mode("sourdough starter", 3, SearchMode::Lexical)
        .await
        .unwrap();
    assert_eq!(hits[0].file_path, "bread.md");
    assert_eq!(hits[0].source, SearchMode::Lexical);
}

#[tokio::test]
async fn hybrid_mode_finds_the_matching_file_first() {
    let dir = tempfile::tempdir().unwrap();
    let index = open(dir.path());
    seed(&index).await;

    let hits = index
        .search_mode("rust borrow checker", 3, SearchMode::Hybrid)
        .await
        .unwrap();
    assert_eq!(hits[0].file_path, "rust.md");
    assert_eq!(hits[0].source, SearchMode::Hybrid);
}

#[tokio::test]
async fn hybrid_scores_are_normalized_and_sorted() {
    let dir = tempfile::tempdir().unwrap();
    let index = open(dir.path());
    seed(&index).await;

    let hits = index
        .search_mode("rust cargo compiler", 5, SearchMode::Hybrid)
        .await
        .unwrap();
    assert!(!hits.is_empty());
    assert!(
        (hits[0].score - 1.0).abs() < 1e-6,
        "le meilleur score hybride vaut 1.0 : {}",
        hits[0].score
    );
    for hit in &hits {
        assert!(
            (0.0..=1.0).contains(&hit.score),
            "score hors [0,1] : {}",
            hit.score
        );
    }
    for pair in hits.windows(2) {
        assert!(pair[0].score >= pair[1].score, "scores non décroissants");
    }
}

#[tokio::test]
async fn lexical_scores_are_normalized() {
    let dir = tempfile::tempdir().unwrap();
    let index = open(dir.path());
    seed(&index).await;

    let hits = index
        .search_mode("rust cargo compiler", 5, SearchMode::Lexical)
        .await
        .unwrap();
    assert!(!hits.is_empty());
    assert!((hits[0].score - 1.0).abs() < 1e-6);
}

#[tokio::test]
async fn hybrid_caps_passages_per_file() {
    let dir = tempfile::tempdir().unwrap();
    let index = open(dir.path());
    // Trois chunks du même fichier contenant tous le terme recherché.
    let long = "alpha beta gamma delta epsilon zeta eta theta ".repeat(4);
    index
        .upsert_file(Path::new("a.md"), &content("a.md", &long, 60))
        .await
        .unwrap();
    index
        .upsert_file(
            Path::new("b.md"),
            &content("b.md", "alpha beta gamma delta epsilon", 1000),
        )
        .await
        .unwrap();
    assert!(
        index.stats().0 >= 4,
        "le fichier a.md doit avoir plusieurs chunks"
    );

    let hits = index
        .search_mode("alpha", 10, SearchMode::Hybrid)
        .await
        .unwrap();
    let from_a = hits.iter().filter(|h| h.file_path == "a.md").count();
    assert!(from_a <= 2, "plafond par fichier dépassé : {from_a}");
}

#[tokio::test]
async fn stopword_only_query_returns_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let index = open(dir.path());
    seed(&index).await;

    let hits = index
        .search_mode("the of and", 5, SearchMode::Hybrid)
        .await
        .unwrap();
    assert!(hits.is_empty());
}

#[tokio::test]
async fn semantic_mode_still_works() {
    let dir = tempfile::tempdir().unwrap();
    let index = open(dir.path());
    seed(&index).await;

    let hits = index
        .search_mode("bread flour yeast", 3, SearchMode::Semantic)
        .await
        .unwrap();
    assert_eq!(hits[0].file_path, "bread.md");
    assert_eq!(hits[0].source, SearchMode::Semantic);
}
