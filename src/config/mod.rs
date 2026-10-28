//! Configuration types — application settings and Drive sync options.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

pub mod store;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct AppSettings {
    #[serde(default = "default_extensions")]
    pub extensions: Vec<String>,

    #[serde(default)]
    pub folders: Vec<String>,

    #[serde(default)]
    pub meeting_notes_setup_done: bool,

    #[serde(default = "default_theme")]
    pub theme: String,

    #[serde(default = "default_splitter")]
    pub splitter_position: f64,

    #[serde(default = "default_h_splitter")]
    pub horizontal_splitter_position: f64,

    #[serde(default = "default_graph_width")]
    pub graph_drawer_width: f64,

    #[serde(default = "default_chat_width")]
    pub chat_column_width: f64,

    #[serde(default)]
    pub chat_column_folded: bool,

    #[serde(default = "default_true")]
    pub auto_save: bool,

    #[serde(default = "default_top_matches")]
    pub top_matches_limit: usize,

    #[serde(default = "default_true")]
    pub ai_enabled: bool,

    #[serde(default = "default_max_file_size")]
    pub max_file_size: usize,

    #[serde(default)]
    pub allow_external_file_overwrite: bool,

    #[serde(default = "default_edge_bands")]
    pub edge_bands: Vec<EdgeBandSettings>,

    #[serde(default = "default_true")]
    pub graph_minimap_visible: bool,

    #[serde(default = "default_true")]
    pub markdown_preview_enabled: bool,

    #[serde(default = "default_true")]
    pub prefer_local_model: bool,

    #[serde(default)]
    pub file_sort_column: FileSortColumn,

    #[serde(default = "default_true")]
    pub file_sort_ascending: bool,

    #[serde(default = "default_true")]
    pub speech_features_enabled: bool,

    #[serde(default)]
    pub tts_voice_name: String,

    #[serde(default = "default_tts_rate")]
    pub tts_rate: i32,

    #[serde(default = "default_tts_volume")]
    pub tts_volume: i32,

    #[serde(default)]
    pub microphone_device_id: String,

    #[serde(default)]
    pub microphone_device_name: String,

    #[serde(default)]
    pub expanded_folders: HashSet<String>,

    #[serde(default)]
    pub drive_sync: DriveSyncSettings,

    #[serde(default)]
    pub walkthrough_completed: bool,

    #[serde(default = "default_true")]
    pub close_to_tray: bool,

    #[serde(default)]
    pub tray_hint_shown: bool,

    #[serde(default)]
    pub tray_balloon_shown: bool,

    #[serde(default)]
    pub start_with_windows: bool,

    #[serde(default = "default_ask_hotkey")]
    pub ask_hotkey: String,

    #[serde(default = "default_dictate_hotkey")]
    pub dictate_hotkey: String,

    #[serde(default)]
    pub meeting_consent_accepted: bool,

    #[serde(default)]
    pub keep_meeting_audio: bool,

    #[serde(default = "default_meeting_lang")]
    pub meeting_language: String,

    #[serde(default)]
    pub chat_thinking_level: ThinkingLevel,

    #[serde(default)]
    pub chat_scope: ChatScope,

    #[serde(default = "default_cloud_context")]
    pub cloud_context_window: usize,
}

fn default_extensions() -> Vec<String> {
    vec![".txt".into(), ".md".into(), ".docx".into(), ".pdf".into()]
}
fn default_theme() -> String { "Dark".into() }
fn default_splitter() -> f64 { 400.0 }
fn default_h_splitter() -> f64 { 200.0 }
fn default_graph_width() -> f64 { 630.0 }
fn default_chat_width() -> f64 { 400.0 }
fn default_true() -> bool { true }
fn default_top_matches() -> usize { 10 }
fn default_max_file_size() -> usize { 10 }
fn default_edge_bands() -> Vec<EdgeBandSettings> { EdgeBandSettings::defaults() }
fn default_tts_rate() -> i32 { 0 }
fn default_tts_volume() -> i32 { 100 }
fn default_ask_hotkey() -> String { "Ctrl+Alt+Space".into() }
fn default_dictate_hotkey() -> String { "Ctrl+Alt+Shift+Space".into() }
fn default_meeting_lang() -> String { "en".into() }
fn default_cloud_context() -> usize { 200_000 }

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            extensions: default_extensions(),
            folders: Vec::new(),
            meeting_notes_setup_done: false,
            theme: default_theme(),
            splitter_position: default_splitter(),
            horizontal_splitter_position: default_h_splitter(),
            graph_drawer_width: default_graph_width(),
            chat_column_width: default_chat_width(),
            chat_column_folded: false,
            auto_save: default_true(),
            top_matches_limit: default_top_matches(),
            ai_enabled: default_true(),
            max_file_size: default_max_file_size(),
            allow_external_file_overwrite: false,
            edge_bands: default_edge_bands(),
            graph_minimap_visible: default_true(),
            markdown_preview_enabled: default_true(),
            prefer_local_model: default_true(),
            file_sort_column: FileSortColumn::Name,
            file_sort_ascending: default_true(),
            speech_features_enabled: default_true(),
            tts_voice_name: String::new(),
            tts_rate: default_tts_rate(),
            tts_volume: default_tts_volume(),
            microphone_device_id: String::new(),
            microphone_device_name: String::new(),
            expanded_folders: HashSet::new(),
            drive_sync: DriveSyncSettings::default(),
            walkthrough_completed: false,
            close_to_tray: default_true(),
            tray_hint_shown: false,
            tray_balloon_shown: false,
            start_with_windows: false,
            ask_hotkey: default_ask_hotkey(),
            dictate_hotkey: default_dictate_hotkey(),
            meeting_consent_accepted: false,
            keep_meeting_audio: false,
            meeting_language: default_meeting_lang(),
            chat_thinking_level: ThinkingLevel::default(),
            chat_scope: ChatScope::default(),
            cloud_context_window: default_cloud_context(),
        }
    }
}

impl AppSettings {
    /// Validates all settings and returns an error if any value is invalid.
    pub fn validate(&self) -> anyhow::Result<()> {
        // Validate splitter positions (must be positive)
        if self.splitter_position <= 0.0 {
            anyhow::bail!("splitter_position must be positive, got {}", self.splitter_position);
        }
        if self.horizontal_splitter_position <= 0.0 {
            anyhow::bail!("horizontal_splitter_position must be positive, got {}", self.horizontal_splitter_position);
        }
        if self.graph_drawer_width <= 0.0 {
            anyhow::bail!("graph_drawer_width must be positive, got {}", self.graph_drawer_width);
        }
        if self.chat_column_width <= 0.0 {
            anyhow::bail!("chat_column_width must be positive, got {}", self.chat_column_width);
        }

        // Validate size limits
        if self.max_file_size == 0 {
            anyhow::bail!("max_file_size must be greater than 0");
        }
        if self.top_matches_limit == 0 {
            anyhow::bail!("top_matches_limit must be greater than 0");
        }
        if self.cloud_context_window == 0 {
            anyhow::bail!("cloud_context_window must be greater than 0");
        }
        if self.cloud_context_window > 1_000_000 {
            anyhow::bail!("cloud_context_window too large (max 1,000,000), got {}", self.cloud_context_window);
        }

        // Validate TTS settings
        if self.tts_volume < 0 || self.tts_volume > 100 {
            anyhow::bail!("tts_volume must be between 0 and 100, got {}", self.tts_volume);
        }
        if self.tts_rate < -500 || self.tts_rate > 500 {
            anyhow::bail!("tts_rate must be between -500 and 500, got {}", self.tts_rate);
        }

        // Validate edge bands
        for (i, band) in self.edge_bands.iter().enumerate() {
            if band.thickness <= 0.0 {
                anyhow::bail!("edge_bands[{}] thickness must be positive, got {}", i, band.thickness);
            }
            if !Self::is_valid_color(&band.color) {
                anyhow::bail!("edge_bands[{}] has invalid color format: '{}' (expected hex like #RRGGBB or rgb(R,G,B))", i, band.color);
            }
        }

        // Validate folders exist
        for (i, folder) in self.folders.iter().enumerate() {
            if !std::path::Path::new(folder).exists() {
                anyhow::bail!("folder[{}] does not exist: {}", i, folder);
            }
        }

        Ok(())
    }

    fn is_valid_color(color: &str) -> bool {
        // Accept hex colors: #RRGGBB, #RRGGBBAA
        if color.starts_with('#') {
            let hex = &color[1..];
            return hex.len() == 6 || hex.len() == 8;
        }
        // Accept rgb(R,G,B) or rgba(R,G,B,A)
        if color.starts_with("rgb(") && color.ends_with(')') {
            let inner = &color[4..color.len()-1];
            let parts: Vec<&str> = inner.split(',').collect();
            if parts.len() == 3 || parts.len() == 4 {
                return parts.iter().all(|p| {
                    p.trim().parse::<u8>().map(|_| true).unwrap_or(false)
                });
            }
        }
        // Accept named colors (basic set)
        let named_colors = ["red", "green", "blue", "black", "white", "yellow", "cyan", "magenta", "transparent"];
        if named_colors.contains(&color.to_lowercase().as_str()) {
            return true;
        }
        false
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum FileSortColumn {
    #[default]
    Name,
    Modified,
    Size,
    Type,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ThinkingLevel {
    None,
    Low,
    #[default]
    Medium,
    High,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ChatScope {
    CurrentFile,
    #[default]
    WatchedFolders,
    AllNotes,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct EdgeBandSettings {
    pub color: String,
    pub thickness: f64,
    pub label: String,
}

impl EdgeBandSettings {
    pub fn defaults() -> Vec<Self> {
        vec![
            Self { color: "#FF6B6B".into(), thickness: 2.0, label: "Strong".into() },
            Self { color: "#4ECDC4".into(), thickness: 1.5, label: "Medium".into() },
            Self { color: "#FFE66D".into(), thickness: 1.0, label: "Weak".into() },
            Self { color: "#A8E6CF".into(), thickness: 0.5, label: "Hint".into() },
        ]
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct DriveSyncSettings {
    pub enabled: bool,
    pub backup_folder_name: String,
    pub last_sync_utc: Option<String>,
    pub conflict_strategy: ConflictStrategy,
}

impl Default for DriveSyncSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            backup_folder_name: "Maieutik Backup".into(),
            last_sync_utc: None,
            conflict_strategy: ConflictStrategy::KeepLocal,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ConflictStrategy {
    #[default]
    KeepLocal,
    KeepRemote,
    KeepBoth,
    Ask,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileItem {
    pub path: PathBuf,
    pub name: String,
    pub size: u64,
    pub modified: chrono::DateTime<chrono::Utc>,
    pub extension: String,
    pub is_watched: bool,
    pub is_indexed: bool,
    pub hash: Option<String>, // blake3
}

impl FileItem {
    pub fn from_path(path: PathBuf) -> anyhow::Result<Self> {
        let meta = std::fs::metadata(&path)?;
        let name = path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let extension = path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("")
            .to_string();
        let modified: chrono::DateTime<chrono::Utc> = meta.modified()?.into();
        Ok(Self {
            path,
            name,
            size: meta.len(),
            modified,
            extension,
            is_watched: false,
            is_indexed: false,
            hash: None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileContent {
    pub path: PathBuf,
    pub text: String,
    pub headings: Vec<Heading>,
    pub chunks: Vec<Chunk>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Heading {
    pub level: usize,
    pub text: String,
    pub byte_offset: usize,
    pub line_number: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chunk {
    pub index: usize,
    pub text: String,
    pub heading_path: Option<String>,
    pub byte_start: usize,
    pub byte_end: usize,
}

