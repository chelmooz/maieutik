//! Tests pour LocalRegistry (P0)

use maieutik::model::{LocalRegistry, ModelBackend, ModelMetadata};
use std::path::PathBuf;
use tempfile::TempDir;

fn test_metadata(name: &str) -> ModelMetadata {
    ModelMetadata {
        name: name.to_string(),
        version: "1.0.0".to_string(),
        sha256: "a".repeat(64),
        size_bytes: 1024,
        source_url: "https://huggingface.co/test".to_string(),
        installed_at: None,
        backend: ModelBackend::Onnx,
        description: None,
        min_free_memory: None,
        local_path: None,
    }
}

#[test]
fn test_registry_persists_metadata() {
    let temp_dir = TempDir::new().expect("temp dir");
    let data_dir = PathBuf::from(temp_dir.path()).join("maieutik");

    let registry = LocalRegistry::new(&data_dir).expect("create registry");

    let metadata = test_metadata("bge-m3");
    registry.add(metadata.clone()).expect("add model");

    let retrieved = registry.get("bge-m3").expect("get model");
    assert_eq!(retrieved, Some(metadata));
}

#[test]
fn test_registry_atomic_write() {
    let temp_dir = TempDir::new().expect("temp dir");
    let data_dir = PathBuf::from(temp_dir.path()).join("maieutik");

    let registry = LocalRegistry::new(&data_dir).expect("create registry");

    registry.add(test_metadata("model-a")).expect("add model-a");
    registry.add(test_metadata("model-b")).expect("add model-b");

    let list = registry.list().expect("list models");
    assert_eq!(list.len(), 2);
    assert!(list.iter().any(|m| m.name == "model-a"));
    assert!(list.iter().any(|m| m.name == "model-b"));
}

#[test]
fn test_registry_concurrent_reads() {
    use std::sync::Arc;

    let temp_dir = TempDir::new().expect("temp dir");
    let data_dir = PathBuf::from(temp_dir.path()).join("maieutik");

    let registry = Arc::new(LocalRegistry::new(&data_dir).expect("create registry"));
    let mut handles = vec![];

    for i in 0..10 {
        let r = registry.clone();
        let handle = std::thread::spawn(move || {
            let metadata = test_metadata(&format!("model-{}", i));
            r.add(metadata).expect("add model");
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("thread panicked");
    }

    let list = registry.list().expect("list models");
    assert_eq!(list.len(), 10);
}

#[test]
fn test_registry_list_returns_all() {
    let temp_dir = TempDir::new().expect("temp dir");
    let data_dir = PathBuf::from(temp_dir.path()).join("maieutik");

    let registry = LocalRegistry::new(&data_dir).expect("create registry");

    registry.add(test_metadata("zebra")).expect("add zebra");
    registry.add(test_metadata("alpha")).expect("add alpha");
    registry.add(test_metadata("beta")).expect("add beta");

    let list = registry.list().expect("list models");
    assert_eq!(list.len(), 3);
    // List should be sorted by name
    assert_eq!(list[0].name, "alpha");
    assert_eq!(list[1].name, "beta");
    assert_eq!(list[2].name, "zebra");
}

#[test]
fn test_registry_remove_deletes_entry() {
    let temp_dir = TempDir::new().expect("temp dir");
    let data_dir = PathBuf::from(temp_dir.path()).join("maieutik");

    let registry = LocalRegistry::new(&data_dir).expect("create registry");

    registry.add(test_metadata("to-remove")).expect("add model");
    assert!(registry.get("to-remove").expect("get").is_some());

    registry.remove("to-remove").expect("remove");
    assert!(registry.get("to-remove").expect("get").is_none());
}

#[test]
fn test_registry_remove_nonexistent() {
    let temp_dir = TempDir::new().expect("temp dir");
    let data_dir = PathBuf::from(temp_dir.path()).join("maieutik");

    let registry = LocalRegistry::new(&data_dir).expect("create registry");

    let result = registry.remove("nonexistent");
    assert!(result.is_err());
}
