//! Tests for config value validation (A8 / Manus B3).
//!
//! Verifies that invalid config values are rejected with contextual errors.

use maieutik::config::{
    AppSettings, ChatScope, ConflictStrategy, DriveSyncSettings, EdgeBandSettings, FileSortColumn,
    ThinkingLevel,
};
use tempfile::TempDir;

/// B8.1 RED: Config validation should reject invalid numerical values
#[test]
fn config_rejects_negative_splitter_position() {
    let mut settings = AppSettings::default();
    settings.splitter_position = -100.0;

    let result = settings.validate();
    assert!(
        result.is_err(),
        "Negative splitter_position should be rejected"
    );
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("splitter_position") || err.contains("splitter"),
        "Error should mention splitter_position: {}",
        err
    );
}

#[test]
fn config_rejects_zero_splitter_position() {
    let mut settings = AppSettings::default();
    settings.splitter_position = 0.0;

    let result = settings.validate();
    assert!(result.is_err(), "Zero splitter_position should be rejected");
}

#[test]
fn config_rejects_negative_max_file_size() {
    let mut settings = AppSettings::default();
    settings.max_file_size = 0;

    let result = settings.validate();
    assert!(result.is_err(), "Zero max_file_size should be rejected");
}

#[test]
fn config_rejects_negative_top_matches_limit() {
    let mut settings = AppSettings::default();
    settings.top_matches_limit = 0;

    let result = settings.validate();
    assert!(result.is_err(), "Zero top_matches_limit should be rejected");
}

#[test]
fn config_rejects_invalid_tts_rate() {
    let mut settings = AppSettings::default();
    settings.tts_rate = -1000; // Too low

    let result = settings.validate();
    assert!(result.is_err(), "Invalid tts_rate should be rejected");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("tts_rate"),
        "Error should mention tts_rate: {}",
        err
    );
}

#[test]
fn config_rejects_invalid_tts_volume() {
    let mut settings = AppSettings::default();
    settings.tts_volume = -50; // Negative

    let result = settings.validate();
    assert!(result.is_err(), "Invalid tts_volume should be rejected");
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("tts_volume"),
        "Error should mention tts_volume: {}",
        err
    );
}

#[test]
fn config_rejects_excessive_tts_volume() {
    let mut settings = AppSettings::default();
    settings.tts_volume = 200; // Over 100%

    let result = settings.validate();
    assert!(result.is_err(), "Excessive tts_volume should be rejected");
}

/// B8.2 RED: Config validation should validate edge bands
#[test]
fn config_rejects_invalid_edge_band_thickness() {
    let mut settings = AppSettings::default();
    settings.edge_bands = vec![EdgeBandSettings {
        color: "#FF6B6B".into(),
        thickness: -1.0, // Invalid
        label: "Strong".into(),
    }];

    let result = settings.validate();
    assert!(
        result.is_err(),
        "Negative edge band thickness should be rejected"
    );
}

#[test]
fn config_rejects_invalid_edge_band_color() {
    let mut settings = AppSettings::default();
    settings.edge_bands = vec![EdgeBandSettings {
        color: "not-a-color".into(),
        thickness: 2.0,
        label: "Strong".into(),
    }];

    let result = settings.validate();
    assert!(result.is_err(), "Invalid color format should be rejected");
}

/// B8.3 RED: Config validation should validate paths
#[test]
fn config_rejects_invalid_folder_paths() {
    let mut settings = AppSettings::default();
    settings.folders = vec!["/non/existent/path/that/definitely/does/not/exist".to_string()];

    let result = settings.validate();
    assert!(
        result.is_err(),
        "Non-existent folder paths should be rejected"
    );
}

/// B8.4 RED: Config validation should validate cloud context window
#[test]
fn config_rejects_zero_cloud_context_window() {
    let mut settings = AppSettings::default();
    settings.cloud_context_window = 0;

    let result = settings.validate();
    assert!(
        result.is_err(),
        "Zero cloud_context_window should be rejected"
    );
}

#[test]
fn config_rejects_excessive_cloud_context_window() {
    let mut settings = AppSettings::default();
    settings.cloud_context_window = 10_000_000; // 10M tokens - too large

    let result = settings.validate();
    assert!(
        result.is_err(),
        "Excessive cloud_context_window should be rejected"
    );
}

/// B8.5 RED: Valid config should pass validation
#[test]
fn config_accepts_valid_settings() {
    let settings = AppSettings::default();

    let result = settings.validate();
    assert!(
        result.is_ok(),
        "Default settings should be valid: {:?}",
        result.err()
    );
}

/// B8.6 RED: Edge band color validation
#[test]
fn config_accepts_valid_hex_colors() {
    let mut settings = AppSettings::default();
    settings.edge_bands = vec![
        EdgeBandSettings {
            color: "#FF6B6B".into(),
            thickness: 2.0,
            label: "Strong".into(),
        },
        EdgeBandSettings {
            color: "#4ECDC4".into(),
            thickness: 1.5,
            label: "Medium".into(),
        },
    ];

    let result = settings.validate();
    assert!(
        result.is_ok(),
        "Valid hex colors should be accepted: {:?}",
        result.err()
    );
}

#[test]
fn config_accepts_valid_rgb_colors() {
    let mut settings = AppSettings::default();
    settings.edge_bands = vec![EdgeBandSettings {
        color: "rgb(255, 107, 107)".into(),
        thickness: 2.0,
        label: "Strong".into(),
    }];

    let result = settings.validate();
    assert!(
        result.is_ok(),
        "Valid RGB colors should be accepted: {:?}",
        result.err()
    );
}
