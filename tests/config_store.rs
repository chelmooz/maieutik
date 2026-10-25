//! Réglages : compatibilité des anciens fichiers et permissions (sans modèle).

use maieutik::config::store::save_settings_to;
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