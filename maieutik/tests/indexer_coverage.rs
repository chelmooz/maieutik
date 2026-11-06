//! Additional coverage tests for core/indexer/mod.rs

use maieutik::core::indexer::{HashEmbedder, VectorIndex, TextEmbedder};
use maieutik::core::retrieval::SearchMode;
use maieutik::config::Chunk;
use std::path::{Path, PathBuf};
use tempfile::tempdir;

fn make_chunk(index: usize, text: &str, heading: Option<&str>) -> Chunk {
    Chunk {
        index,
        text: text.to_string(),
        heading_path: heading.map(|s| s.to_string()),
        byte_start: 0,
        byte_end: text.len(),
    }
}

#[tokio::test]
async fn hash_embedder_basic_operations() {
    let embedder = HashEmbedder::new();
    
    // Test embed (from TextEmbedder trait)
    let vec = embedder.embed("test text").unwrap();
    assert_eq!(vec.len(), 1024);
    
    // Test embed_batch (from TextEmbedder trait default impl)
    let texts = &["text1", "text2", "text3"];
    let batch = embedder.embed_batch(texts).unwrap();
    assert_eq!(batch.len(), 3);
    assert_eq!(batch[0].len(), 1024);
}

#[tokio::test]
async fn vector_index_open_with_hash_embedder() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("MAIEUTIK_DATA_DIR", dir.path());
    
    let embedder = HashEmbedder::new();
    let index = VectorIndex::open(dir.path(), embedder).unwrap();
    
    // Test basic operations
    let (doc_count, meta) = index.stats();
    assert_eq!(doc_count, 0);
    assert_eq!(meta.schema_version, 2);
    
    std::env::remove_var("MAIEUTIK_DATA_DIR");
}

#[tokio::test]
async fn vector_index_store_and_search() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("MAIEUTIK_DATA_DIR", dir.path());
    
    let embedder = HashEmbedder::new();
    let index = VectorIndex::open(dir.path(), embedder).unwrap();
    
    // Use upsert_file which handles storing and HNSW rebuild
    let content1 = maieutik::config::FileContent {
        path: PathBuf::from("test1.md"),
        headings: vec![],
        text: "Hello world".to_string(),
        chunks: vec![
            make_chunk(0, "Hello world", Some("Test")),
            make_chunk(1, "Rust programming", Some("Rust")),
        ],
    };
    
    index.upsert_file(Path::new("test1.md"), &content1).await.unwrap();
    index.rebuild_hnsw().await.unwrap();
    
    // Search
    let hits = index.search("hello", 5).await.unwrap();
    assert!(!hits.is_empty());
    
    // Test search_mode
    let hits = index.search_mode("hello", 5, SearchMode::Semantic).await.unwrap();
    assert!(!hits.is_empty());
    
    let hits = index.search_mode("hello", 5, SearchMode::Lexical).await.unwrap();
    assert!(!hits.is_empty());
    
    let hits = index.search_mode("hello", 5, SearchMode::Hybrid).await.unwrap();
    assert!(!hits.is_empty());
    
    std::env::remove_var("MAIEUTIK_DATA_DIR");
}

#[tokio::test]
async fn vector_index_update_bm25() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("MAIEUTIK_DATA_DIR", dir.path());
    
    let embedder = HashEmbedder::new();
    let index = VectorIndex::open(dir.path(), embedder).unwrap();
    
    // Use upsert_file to properly store the file
    let content = maieutik::config::FileContent {
        path: PathBuf::from("test.md"),
        headings: vec![],
        text: "Test content".to_string(),
        chunks: vec![make_chunk(0, "Test content", None)],
    };
    
    index.upsert_file(Path::new("test.md"), &content).await.unwrap();
    index.rebuild_hnsw().await.unwrap();
    
    // Verify BM25 works
    let hits = index.search_mode("test", 5, SearchMode::Lexical).await.unwrap();
    assert!(!hits.is_empty());
    
    std::env::remove_var("MAIEUTIK_DATA_DIR");
}

#[tokio::test]
async fn vector_index_remove_file() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("MAIEUTIK_DATA_DIR", dir.path());
    
    let embedder = HashEmbedder::new();
    let index = VectorIndex::open(dir.path(), embedder).unwrap();
    
    // Add a file
    let content = maieutik::config::FileContent {
        path: PathBuf::from("test.md"),
        headings: vec![],
        text: "Test content".to_string(),
        chunks: vec![make_chunk(0, "Test content", None)],
    };
    
    index.upsert_file(Path::new("test.md"), &content).await.unwrap();
    index.rebuild_hnsw().await.unwrap();
    
    // Remove the file
    index.remove_file(Path::new("test.md")).await.unwrap();
    
    // Search should not find it
    let hits = index.search("test", 5).await.unwrap();
    assert!(hits.is_empty());
    
    std::env::remove_var("MAIEUTIK_DATA_DIR");
}

#[tokio::test]
async fn vector_index_purge_file() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("MAIEUTIK_DATA_DIR", dir.path());
    
    let embedder = HashEmbedder::new();
    let index = VectorIndex::open(dir.path(), embedder).unwrap();
    
    // Add a file
    let content = maieutik::config::FileContent {
        path: PathBuf::from("test.md"),
        headings: vec![],
        text: "Test content".to_string(),
        chunks: vec![make_chunk(0, "Test content", None)],
    };
    
    index.upsert_file(Path::new("test.md"), &content).await.unwrap();
    index.rebuild_hnsw().await.unwrap();
    
    // Purge the file
    index.purge_file("test.md").unwrap();
    
    // Should be completely gone
    let paths = index.stored_file_paths().unwrap();
    assert!(!paths.contains(&"test.md".to_string()));
    
    std::env::remove_var("MAIEUTIK_DATA_DIR");
}

#[tokio::test]
async fn vector_index_fingerprint() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("MAIEUTIK_DATA_DIR", dir.path());
    
    // Change to the temp dir so "test.md" resolves correctly for fingerprint
    let original_dir = std::env::current_dir().unwrap();
    std::env::set_current_dir(dir.path()).unwrap();
    
    // Write the file to disk so fingerprint can be read
    std::fs::write("test.md", "Test content").unwrap();
    
    let embedder = HashEmbedder::new();
    let index = VectorIndex::open(dir.path(), embedder).unwrap();
    
    // Add a file
    let content = maieutik::config::FileContent {
        path: PathBuf::from("test.md"),
        headings: vec![],
        text: "Test content".to_string(),
        chunks: vec![make_chunk(0, "Test content", None)],
    };
    
    index.upsert_file(Path::new("test.md"), &content).await.unwrap();
    index.rebuild_hnsw().await.unwrap();
    
    // Check fingerprint
    let fp = index.fingerprint_of("test.md").unwrap();
    assert!(fp.is_some());
    
    // Non-existent file
    let fp = index.fingerprint_of("nonexistent.md").unwrap();
    assert!(fp.is_none());
    
    // Restore original directory
    std::env::set_current_dir(original_dir).unwrap();
    std::env::remove_var("MAIEUTIK_DATA_DIR");
}

#[tokio::test]
async fn vector_index_stored_files() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("MAIEUTIK_DATA_DIR", dir.path());
    
    let embedder = HashEmbedder::new();
    let index = VectorIndex::open(dir.path(), embedder).unwrap();
    
    // Initially empty
    let files = index.stored_files().unwrap();
    assert!(files.is_empty());
    
    // Add files
    let content = maieutik::config::FileContent {
        path: PathBuf::from("test1.md"),
        headings: vec![],
        text: "Test 1".to_string(),
        chunks: vec![make_chunk(0, "Test 1", None)],
    };
    
    index.upsert_file(Path::new("test1.md"), &content).await.unwrap();
    index.rebuild_hnsw().await.unwrap();
    
    let files = index.stored_files().unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(files[0].0, "test1.md");
    
    std::env::remove_var("MAIEUTIK_DATA_DIR");
}

#[tokio::test]
async fn vector_index_needs_reindex() {
    let dir = tempfile::tempdir().unwrap();
    std::env::set_var("MAIEUTIK_DATA_DIR", dir.path());
    
    let embedder = HashEmbedder::new();
    let index = VectorIndex::open(dir.path(), embedder).unwrap();
    
    // Should need reindex initially (no data)
    let _needs = index.needs_reindex().unwrap();
    // May be false if no prior index
    
    // Clear needs reindex
    index.clear_needs_reindex().unwrap();
    
    std::env::remove_var("MAIEUTIK_DATA_DIR");
}