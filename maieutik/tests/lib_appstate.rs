//! Tests for lib.rs AppState and watcher functions (sans modèle).

use maieutik::{
    config::store::{data_dir, ConfigStore},
    core::indexer::{HashEmbedder, VectorIndex},
    core::scan::index_path,
    core::watch::{is_indexable_file, DebouncedProcessor, FileWatcher, WatchEvent},
    AppState, save_settings_if_changed,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tempfile::tempdir;
use tokio::sync::{mpsc, RwLock};

#[tokio::test]
async fn app_state_new_works() {
    let state = AppState::new().await;
    assert!(state.is_ok());
    let _state = state.unwrap();
    assert!(true); // config is internal
}

#[tokio::test]
async fn app_state_initialize_indexer_fails_without_models() {
    let dir = tempdir().unwrap();
    let data_path = dir.path().join("data");
    std::fs::create_dir_all(&data_path).unwrap();

    std::env::set_var("MAIEUTIK_DATA_DIR", &data_path);

    let mut state = AppState::new().await.unwrap();

    // Test that it fails gracefully without models
    let model_path = PathBuf::from("/nonexistent/model.onnx");
    let tokenizer_path = PathBuf::from("/nonexistent/tokenizer.json");
    let result = state.initialize_indexer(model_path, tokenizer_path).await;
    assert!(result.is_err());

    std::env::remove_var("MAIEUTIK_DATA_DIR");
}

#[tokio::test]
async fn save_settings_if_changed_compiles() {
    // Just verify the function exists and compiles
    let settings = maieutik::config::AppSettings::default();
    let _ = save_settings_if_changed(&settings);
}

#[tokio::test]
async fn is_indexable_file_respects_extension_filter() {
    let dir = tempdir().unwrap();
    let md_file = dir.path().join("test.md");
    let txt_file = dir.path().join("test.txt");
    let pdf_file = dir.path().join("test.pdf");
    let docx_file = dir.path().join("test.docx");
    let other_file = dir.path().join("test.xyz");

    std::fs::write(&md_file, "test").unwrap();
    std::fs::write(&txt_file, "test").unwrap();
    std::fs::write(&pdf_file, "test").unwrap();
    std::fs::write(&docx_file, "test").unwrap();
    std::fs::write(&other_file, "test").unwrap();

    let settings = maieutik::config::AppSettings::default();

    // Default extensions: .txt, .md, .docx, .pdf
    assert!(is_indexable_file(&md_file, &settings).unwrap());
    assert!(is_indexable_file(&txt_file, &settings).unwrap());
    assert!(is_indexable_file(&pdf_file, &settings).unwrap());
    assert!(is_indexable_file(&docx_file, &settings).unwrap());
    assert!(!is_indexable_file(&other_file, &settings).unwrap());
}

#[tokio::test]
async fn is_indexable_file_respects_size_limit() {
    let dir = tempdir().unwrap();
    let small_file = dir.path().join("small.md");
    let large_file = dir.path().join("large.md");

    std::fs::write(&small_file, "small").unwrap();
    std::fs::write(&large_file, vec![b'x'; 1024 * 1024 * 15]).unwrap(); // 15 MB

    let mut settings = maieutik::config::AppSettings::default();
    settings.max_file_size = 10; // 10 MB

    assert!(is_indexable_file(&small_file, &settings).unwrap());
    assert!(!is_indexable_file(&large_file, &settings).unwrap());
}

#[tokio::test]
async fn debounced_processor_handles_events() {
    let (tx, rx) = mpsc::channel(100);
    let (process_tx, _process_rx) = mpsc::channel(100);

    let processor = DebouncedProcessor::new(rx, process_tx);
    let handle = tokio::spawn(processor.run());

    let temp_dir = tempdir().unwrap();
    let file1 = temp_dir.path().join("test1.md");
    let file2 = temp_dir.path().join("test2.md");
    std::fs::write(&file1, "test1").unwrap();
    std::fs::write(&file2, "test2").unwrap();

    tx.send(WatchEvent::Created(file1.clone())).await.unwrap();
    tx.send(WatchEvent::Modified(file2.clone())).await.unwrap();
    tx.send(WatchEvent::Removed(file1.clone())).await.unwrap();

    // Close the channel to stop the processor
    drop(tx);

    // Wait for processor to finish
    handle.await.unwrap();
}

#[tokio::test]
async fn file_watcher_creation_works() {
    let settings = Arc::new(RwLock::new(maieutik::config::AppSettings::default()));
    let result = FileWatcher::new(settings, mpsc::channel(100).0);
    // The watcher creation itself doesn't validate folders until watch() is called
    assert!(result.is_ok());
}

#[tokio::test]
async fn watch_event_variants() {
    let temp_dir = tempdir().unwrap();
    let file1 = temp_dir.path().join("test1.md");
    let file2 = temp_dir.path().join("test2.md");
    std::fs::write(&file1, "test1").unwrap();
    std::fs::write(&file2, "test2").unwrap();

    let created = WatchEvent::Created(file1.clone());
    let modified = WatchEvent::Modified(file1.clone());
    let removed = WatchEvent::Removed(file1.clone());
    let renamed = WatchEvent::Renamed {
        from: file1.clone(),
        to: file2.clone(),
    };

    match created {
        WatchEvent::Created(p) => assert_eq!(p, file1),
        _ => panic!("expected Created"),
    }
    match modified {
        WatchEvent::Modified(p) => assert_eq!(p, file1),
        _ => panic!("expected Modified"),
    }
    match removed {
        WatchEvent::Removed(p) => assert_eq!(p, file1),
        _ => panic!("expected Removed"),
    }
    match renamed {
        WatchEvent::Renamed { from, to } => {
            assert_eq!(from, file1);
            assert_eq!(to, file2);
        }
        _ => panic!("expected Renamed"),
    }
}