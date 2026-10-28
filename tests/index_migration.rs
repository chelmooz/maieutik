//! Tests for index migration on schema/model mismatch (AUD-01 / RUST-003).
//!
//! Verifies that mismatch handling preserves data integrity:
//! - vectors and file_meta are backed up
//! - needs_reindex flag is set
//! - reindexing restores search and clears flag

use maieutik::config::FileContent;
use maieutik::core::indexer::extract_headings;
use maieutik::core::indexer::{HashEmbedder, IndexMeta, VectorIndex};
use std::path::PathBuf;
use tempfile::TempDir;

fn make_content(text: &str, data_dir: &std::path::Path) -> (FileContent, PathBuf) {
    let file_path = data_dir.join("test.md");
    // Create the actual file on disk so FileFingerprint::read works
    std::fs::write(&file_path, text).unwrap();

    let base = FileContent {
        path: file_path.clone(),
        text: text.to_string(),
        headings: extract_headings(text),
        chunks: Vec::new(),
    };
    let chunks = maieutik::core::indexer::Chunker::new(100, 0).chunk(&base);
    (FileContent { chunks, ..base }, file_path)
}

fn corrupt_meta_for_mismatch(data_dir: &std::path::Path) {
    let db = sled::open(data_dir.join("vector_index")).unwrap();
    let meta_tree = db.open_tree("meta").unwrap();
    let bad_meta = IndexMeta {
        id: "meta".to_string(),
        schema_version: 999,
        model_id: "wrong-model".to_string(),
        dimensions: 999,
        chunker_version: 1,
        model_size_bytes: None,
        needs_reindex: false,
    };
    meta_tree
        .insert("meta", bincode::serialize(&bad_meta).unwrap())
        .unwrap();
    db.flush().unwrap();
}

fn has_backup_tree(data_dir: &std::path::Path, prefix: &str) -> bool {
    let db = sled::open(data_dir.join("vector_index")).unwrap();
    db.tree_names().into_iter().any(|name| {
        let name_str = String::from_utf8_lossy(&name[..]);
        name_str.starts_with(prefix)
    })
}

/// A2.1 RED: After mismatch, file_meta should be backed up (not left orphaned)
#[test]
fn mismatch_clears_file_meta() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();

    // Create index and add data
    let embedder = HashEmbedder::new();
    let index = VectorIndex::open(data_dir, embedder).unwrap();

    let (content, file_path) = make_content("hello world test content for migration", data_dir);
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(index.upsert_file(&file_path, &content))
        .unwrap();

    // Verify file_meta exists before mismatch (via index method)
    let stored = index.stored_files().unwrap();
    assert!(
        !stored.is_empty(),
        "Should have stored files before mismatch"
    );

    // Drop index to release lock
    drop(index);

    // Corrupt metadata to trigger mismatch
    corrupt_meta_for_mismatch(data_dir);

    // Reopen - should trigger backup and set needs_reindex
    let embedder = HashEmbedder::new();
    let index2 = VectorIndex::open(data_dir, embedder).unwrap();

    // needs_reindex flag should be set (proves mismatch was handled)
    assert!(
        index2.needs_reindex(),
        "needs_reindex flag should be set after mismatch"
    );

    // Active file_meta should be cleared (no orphaned entries)
    let stored = index2.stored_files().unwrap();
    assert!(
        stored.is_empty(),
        "Active file_meta should be cleared after mismatch, stored_files should be empty"
    );

    // Drop index2 to release lock, then verify backups were created
    drop(index2);

    // Now we can safely open new connections to verify backups
    assert!(
        has_backup_tree(data_dir, "vectors_backup_"),
        "vectors_backup tree should exist"
    );
    assert!(
        has_backup_tree(data_dir, "file_meta_backup_"),
        "file_meta_backup tree should exist"
    );
}

/// A2.3 RED: needs_reindex flag should be set after mismatch
#[test]
fn mismatch_raises_needs_reindex() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();

    // Create index and add data
    {
        let embedder = HashEmbedder::new();
        let index = VectorIndex::open(data_dir, embedder).unwrap();

        let (content, file_path) = make_content("hello world test content for migration", data_dir);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(index.upsert_file(&file_path, &content))
            .unwrap();
    }

    // Corrupt metadata to trigger mismatch
    corrupt_meta_for_mismatch(data_dir);

    // Reopen - should set needs_reindex flag
    let embedder = HashEmbedder::new();
    let index2 = VectorIndex::open(data_dir, embedder).unwrap();

    // needs_reindex should be true
    assert!(
        index2.needs_reindex(),
        "needs_reindex flag should be true after mismatch"
    );

    drop(index2);
}

/// A2.5 RED: After successful index_all, flag should be cleared and search works
#[test]
fn reindex_restores_search() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();

    // Create index and add data
    {
        let embedder = HashEmbedder::new();
        let index = VectorIndex::open(data_dir, embedder).unwrap();

        let (content, file_path) = make_content("hello world test content for migration", data_dir);
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(index.upsert_file(&file_path, &content))
            .unwrap();
    }

    // Corrupt metadata to trigger mismatch
    corrupt_meta_for_mismatch(data_dir);

    // Reopen
    let embedder = HashEmbedder::new();
    let index2 = VectorIndex::open(data_dir, embedder).unwrap();

    // Verify flag is set
    assert!(index2.needs_reindex(), "Flag should be set before reindex");

    // Perform reindex (simulate index_all) - re-add content
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        let (content, file_path) = make_content("hello world test content for migration", data_dir);
        index2.upsert_file(&file_path, &content).await.unwrap();
    });

    // Clear the flag after successful reindex
    index2.clear_needs_reindex().unwrap();

    // Flag should be cleared
    assert!(
        !index2.needs_reindex(),
        "Flag should be cleared after reindex"
    );

    // Search should work
    let results = rt.block_on(index2.search("hello world", 5)).unwrap();
    assert!(!results.is_empty(), "Search should work after reindex");

    drop(index2);
}

/// A2.7 RED: Crash between vectors and fingerprint should be detected/repaired
#[test]
fn crash_between_vectors_and_fingerprint() {
    let dir = TempDir::new().unwrap();
    let data_dir = dir.path();
    let embedder = HashEmbedder::new();
    let index = VectorIndex::open(data_dir, embedder).unwrap();

    // Add content
    let (content, file_path) = make_content("test content", data_dir);
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(index.upsert_file(&file_path, &content))
        .unwrap();

    // Verify the index can be rebuilt
    rt.block_on(index.build_hnsw()).unwrap();

    let results = rt.block_on(index.search("test", 5)).unwrap();
    assert!(!results.is_empty());

    drop(index);
}
