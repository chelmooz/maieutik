//! Tests for config concurrent writes and migration (AUD-03).

use maieutik::config::store::{load_settings, save_settings_to};
use maieutik::config::AppSettings;
use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;
use tempfile::TempDir;

fn default_settings_json() -> String {
    r###"{
  "version": 1,
  "settings": {
    "extensions": [".txt", ".md", ".docx", ".pdf"],
    "folders": [],
    "meeting_notes_setup_done": false,
    "theme": "Dark",
    "splitter_position": 400.0,
    "horizontal_splitter_position": 200.0,
    "graph_drawer_width": 630.0,
    "chat_column_width": 400.0,
    "chat_column_folded": false,
    "auto_save": true,
    "top_matches_limit": 10,
    "ai_enabled": true,
    "max_file_size": 10,
    "allow_external_file_overwrite": false,
    "edge_bands": [
      {"color": "#FF6B6B", "thickness": 2.0, "label": "Strong"},
      {"color": "#4ECDC4", "thickness": 1.5, "label": "Medium"},
      {"color": "#FFE66D", "thickness": 1.0, "label": "Weak"},
      {"color": "#A8E6CF", "thickness": 0.5, "label": "Hint"}
    ],
    "graph_minimap_visible": true,
    "markdown_preview_enabled": true,
    "prefer_local_model": true,
    "file_sort_column": "Name",
    "file_sort_ascending": true,
    "speech_features_enabled": true,
    "tts_voice_name": "",
    "tts_rate": 0,
    "tts_volume": 100,
    "microphone_device_id": "",
    "microphone_device_name": "",
    "expanded_folders": [],
    "drive_sync": {
      "enabled": false,
      "backup_folder_name": "Maieutik Backup",
      "last_sync_utc": null,
      "conflict_strategy": "KeepLocal"
    },
    "walkthrough_completed": false,
    "close_to_tray": true,
    "tray_hint_shown": false,
    "tray_balloon_shown": false,
    "start_with_windows": false,
    "ask_hotkey": "Ctrl+Alt+Space",
    "dictate_hotkey": "Ctrl+Alt+Shift+Space",
    "meeting_consent_accepted": false,
    "keep_meeting_audio": false,
    "meeting_language": "en",
    "chat_thinking_level": "Medium",
    "chat_scope": "WatchedFolders",
    "cloud_context_window": 200000
  }
}"###
        .to_string()
}

fn make_versioned_json(version: u32) -> String {
    let base = default_settings_json();
    // Replace version 1 with the desired version
    base.replacen(
        r###""version": 1"###,
        &format!(r###""version": {}"###, version),
        1,
    )
}

/// A4.1 RED: Concurrent saves should not race (deterministic tmp name causes race)
#[test]
fn config_concurrent_saves_no_race() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");
    let settings = AppSettings::default();

    let num_threads = 2;
    let iterations = 20;
    let barrier = Arc::new(Barrier::new(num_threads));
    let mut handles = vec![];

    for _ in 0..num_threads {
        let path = config_path.clone();
        let settings = settings.clone();
        let barrier = Arc::clone(&barrier);

        let handle = thread::spawn(move || {
            barrier.wait(); // Synchronize start
            for _ in 0..iterations {
                let _ = save_settings_to(&path, &settings);
            }
        });
        handles.push(handle);
    }

    // Wait for all threads
    for h in handles {
        h.join().unwrap();
    }

    // Verify the config file was written correctly (read directly)
    let content = fs::read_to_string(&config_path).unwrap_or_else(|e| {
        panic!(
            "Config file not found or corrupted by concurrent writes: {}",
            e
        );
    });
    eprintln!("Config content length: {}", content.len());
    let loaded: serde_json::Value = serde_json::from_str(&content).unwrap_or_else(|_| {
        panic!("Config file corrupted by concurrent writes - invalid JSON");
    });
    // Check that the version is correct
    assert_eq!(loaded["version"], 1);
    let extensions: Vec<String> = loaded["settings"]["Extensions"]
        .as_array()
        .unwrap_or_else(|| {
            panic!(
                "Extensions not an array: {:?}",
                loaded["settings"]["Extensions"]
            );
        })
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(extensions, settings.extensions);
}

/// A4.2 RED: Config migration - old version should be handled gracefully
#[test]
fn config_migration_old_version() {
    // Use the actual config path that load_settings() will read from
    let config_path = maieutik::config::store::config_path().unwrap();

    // Backup existing config if it exists
    let backup = if config_path.exists() {
        Some(fs::read_to_string(&config_path).unwrap())
    } else {
        None
    };

    // Write old version config (version 0)
    let old_json = make_versioned_json(0);
    fs::write(&config_path, old_json).unwrap();

    let result = load_settings();

    // Restore original config
    if let Some(backup) = backup {
        fs::write(&config_path, backup).unwrap();
    } else {
        let _ = fs::remove_file(&config_path);
    }

    // Should handle old version gracefully (currently accepts with warning)
    assert!(
        result.is_ok(),
        "Old version should be accepted with migration warning"
    );
}

/// A4.3 RED: Config migration - future version should be rejected explicitly
#[test]
fn config_migration_future_version_rejected() {
    // Use the actual config path that load_settings() will read from
    let config_path = maieutik::config::store::config_path().unwrap();

    // Backup existing config if it exists
    let backup = if config_path.exists() {
        Some(fs::read_to_string(&config_path).unwrap())
    } else {
        None
    };

    // Write future version config (version 999)
    let future_json = make_versioned_json(999);
    fs::write(&config_path, future_json).unwrap();

    let result = load_settings();

    // Restore original config
    if let Some(backup) = backup {
        fs::write(&config_path, backup).unwrap();
    } else {
        let _ = fs::remove_file(&config_path);
    }

    // Should reject with clear error
    assert!(result.is_err(), "Future version should be rejected");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("version") || err.contains("newer"),
        "Error should mention version/newer: {}",
        err
    );
}

/// A4.4 RED: Config migration - invalid JSON should be rejected
#[test]
fn config_invalid_json_rejected() {
    // Use the actual config path that load_settings() will read from
    let config_path = maieutik::config::store::config_path().unwrap();

    // Backup existing config if it exists
    let backup = if config_path.exists() {
        Some(fs::read_to_string(&config_path).unwrap())
    } else {
        None
    };

    // Write invalid JSON
    fs::write(&config_path, "{ invalid json }").unwrap();

    let result = load_settings();

    // Restore original config
    if let Some(backup) = backup {
        fs::write(&config_path, backup).unwrap();
    } else {
        let _ = fs::remove_file(&config_path);
    }

    assert!(result.is_err(), "Invalid JSON should be rejected");
}

/// A4.5 RED: No leftover tmp files after concurrent saves
#[test]
fn config_no_leftover_tmp_files() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join("config.json");
    let settings = AppSettings::default();

    let num_threads = 2;
    let iterations = 10;
    let barrier = Arc::new(Barrier::new(num_threads));
    let mut handles = vec![];

    for _ in 0..num_threads {
        let path = config_path.clone();
        let settings = settings.clone();
        let barrier = Arc::clone(&barrier);

        let handle = thread::spawn(move || {
            barrier.wait();
            for _ in 0..iterations {
                let _ = save_settings_to(&path, &settings);
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }

    // Check for leftover .tmp files
    let entries: Vec<_> = fs::read_dir(dir.path()).unwrap().collect();
    let tmp_files: Vec<_> = entries
        .iter()
        .filter_map(|e| e.as_ref().ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();

    assert!(
        tmp_files.is_empty(),
        "Found leftover tmp files: {:?}",
        tmp_files
    );
}
