//! Tests TDD pour SecretsStore (P0)
//!
//! Exécuter avec : cargo test --package maieutik secrets_store

use maieutik::secrets::SecretsStore;
use std::sync::Arc;
use tempfile::TempDir;

// ============================================================================
// Test 1: Object safety du trait
// ============================================================================
#[tokio::test]
async fn test_secrets_store_trait_object_safety() {
    // Box<dyn SecretsStore> doit être compilable
    let _: Option<Arc<dyn SecretsStore>> = None;
}

// ============================================================================
// Test 2: Persistance entre instances
// ============================================================================
#[tokio::test]
async fn test_file_store_persists_across_instances() {
    let temp_dir = TempDir::new().expect("temp dir");
    let file = temp_dir.path().join("secrets.json");

    // Premier store
    {
        let store = maieutik::secrets::FileStore::with_path(file.clone())
            .await
            .expect("create store");
        store.set("test_key", "test_value").await.expect("set key");
    }

    // Deuxième store - doit retrouver la valeur
    {
        let store = maieutik::secrets::FileStore::with_path(file.clone())
            .await
            .expect("create store");
        let value = store.get("test_key").await.expect("get key");
        assert_eq!(value, Some("test_value".to_string()));
    }
}

// ============================================================================
// Test 3: Écriture atomique
// ============================================================================
#[tokio::test]
async fn test_file_store_atomic_write() {
    let temp_dir = TempDir::new().expect("temp dir");
    let file = temp_dir.path().join("secrets.json");

    let store = maieutik::secrets::FileStore::with_path(file.clone())
        .await
        .expect("create store");
    store.set("key1", "value1").await.expect("set key1");
    store.set("key2", "value2").await.expect("set key2");

    // Vérifier les deux clés
    assert_eq!(
        store.get("key1").await.expect("get key1"),
        Some("value1".to_string())
    );
    assert_eq!(
        store.get("key2").await.expect("get key2"),
        Some("value2".to_string())
    );
}

// ============================================================================
// Test 4: Permissions 0600
// ============================================================================
#[tokio::test]
async fn test_file_store_permissions_0600() {
    let temp_dir = TempDir::new().expect("temp dir");
    let file = temp_dir.path().join("secrets.json");

    let store = maieutik::secrets::FileStore::with_path(file.clone())
        .await
        .expect("create store");
    let _ = store.set("key", "value").await;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = std::fs::metadata(&file).expect("metadata");
        let perm = metadata.permissions().mode();
        assert_eq!(
            perm & 0o777,
            0o600,
            "File should have 0600 permissions, got {:o}",
            perm & 0o777
        );
    }
}

// ============================================================================
// Test 5: List des clés
// ============================================================================
#[tokio::test]
async fn test_file_store_list_keys() {
    let temp_dir = TempDir::new().expect("temp dir");
    let file = temp_dir.path().join("secrets.json");

    let store = maieutik::secrets::FileStore::with_path(file.clone())
        .await
        .expect("create store");
    store.set("alpha", "a").await.expect("set alpha");
    store.set("beta", "b").await.expect("set beta");

    let keys = store.list().await.expect("list keys");
    assert!(keys.contains(&"alpha".to_string()));
    assert!(keys.contains(&"beta".to_string()));
    assert_eq!(keys.len(), 2);
}

// ============================================================================
// Test 6: Suppression
// ============================================================================
#[tokio::test]
async fn test_file_store_delete_then_get_none() {
    let temp_dir = TempDir::new().expect("temp dir");
    let file = temp_dir.path().join("secrets.json");

    let store = maieutik::secrets::FileStore::with_path(file.clone())
        .await
        .expect("create store");
    store.set("to_delete", "value").await.expect("set key");
    store.delete("to_delete").await.expect("delete key");

    let value = store.get("to_delete").await.expect("get key");
    assert_eq!(value, None);
}

// ============================================================================
// Test 7: Clé manquante
// ============================================================================
#[tokio::test]
async fn test_file_store_get_missing_returns_none() {
    let temp_dir = TempDir::new().expect("temp dir");
    let file = temp_dir.path().join("secrets.json");

    let store = maieutik::secrets::FileStore::with_path(file.clone())
        .await
        .expect("create store");
    let value = store.get("missing_key").await.expect("get missing key");
    assert_eq!(value, None);
}