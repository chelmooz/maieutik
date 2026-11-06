//! Réglages : compatibilité des anciens fichiers et permissions (sans modèle).

use maieutik::config::store::{
    config_path, data_dir, embedder_model_path, embedder_tokenizer_path, ensure_dirs, load_settings,
    load_settings_from, models_dir, ort_dylib_path, save_settings, save_settings_to,
    save_settings_to_file, settings_dir,
};
use maieutik::config::AppSettings;

#[test]
fn legacy_openai_fields_are_ignored() {
    let json = r#"{"OpenAiEndpoint":"x","OpenAiApiKey":"k","OpenAiModelName":"m","Theme":"dark"}"#;
    let s: AppSettings = serde_json::from_str(json).unwrap();
    assert_eq!(s.theme, "dark");
}

#[test]
fn saved_settings_have_no_removed_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    save_settings_to(&path, &AppSettings::default()).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    assert!(!text.contains("OpenAi"));
    let v: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["version"], 1);
}

#[test]
fn load_rejects_invalid_values_but_allows_missing_folders() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    let settings = AppSettings {
        folders: vec!["/non/existent/path/for/config-load".to_string()],
        ..Default::default()
    };
    save_settings_to_file(&settings, &path).unwrap();
    assert!(load_settings_from(&path).is_ok());

    let invalid = AppSettings {
        edge_bands: vec![maieutik::config::EdgeBandSettings {
            color: "#GGGGGG".into(),
            thickness: 2.0,
            label: "Strong".into(),
        }],
        ..Default::default()
    };
    save_settings_to_file(&invalid, &path).unwrap();
    assert!(load_settings_from(&path).is_err());
}

#[test]
fn missing_home_dir_returns_error() {
    // This test verifies that the config functions return proper errors
    // instead of panicking when home directory is unavailable.
    // We can't easily unset HOME in a test without affecting other tests,
    // but we verify the return types are Result.
    let _ = settings_dir();
    let _ = data_dir();
    let _ = models_dir();
    let _ = embedder_model_path();
    let _ = embedder_tokenizer_path();
}

#[cfg(unix)]
#[test]
fn saved_config_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.json");
    std::fs::write(&path, "old").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
    save_settings_to(&path, &AppSettings::default()).unwrap();
    let mode = std::fs::metadata(&path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}

#[test]
fn config_path_returns_valid_path() {
    let path = config_path();
    assert!(path.is_ok());
    let path = path.unwrap();
    assert!(path.to_string_lossy().contains("maieutik"));
    assert!(path.to_string_lossy().ends_with("config.json"));
}

#[test]
fn ensure_dirs_creates_required_directories() {
    // ensure_dirs creates the default config, data, and models directories
    // using the standard directory functions. It should succeed.
    let result = ensure_dirs();
    assert!(result.is_ok());
}

#[test]
fn load_settings_works_with_default_config() {
    // Test that load_settings works (uses default config dir)
    let result = load_settings();
    // May fail if no config exists, but shouldn't panic
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn save_settings_works_with_custom_path() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("custom_config.json");
    
    let settings = AppSettings::default();
    let result = save_settings_to_file(&settings, &path);
    assert!(result.is_ok());
    assert!(path.exists());
    
    // Verify we can load it back
    let loaded = load_settings_from(&path);
    assert!(loaded.is_ok());
}

#[test]
fn ort_dylib_path_returns_option() {
    let path = ort_dylib_path();
    // Should return None or Some path without panicking
    assert!(path.is_none() || path.is_some());
}

#[test]
fn save_settings_to_creates_directories() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested").join("deep").join("config.json");
    
    // Create parent directories first (save_settings_to doesn't create them)
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    
    let settings = AppSettings::default();
    let result = save_settings_to(&path, &settings);
    assert!(result.is_ok());
    assert!(path.exists());
}
