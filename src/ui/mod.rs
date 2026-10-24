//! UI module.
//!
//! The graphical interface is not implemented yet: the egui/tauri dependencies
//! are disabled in `Cargo.toml`, so the binary runs headless (CLI, MCP server,
//! watcher). `run_ui` stays as the single entry point the binary calls.

pub mod egui_app {
    use anyhow::Result;
    use crate::AppState;
    use std::sync::Arc;

    pub async fn run_ui(_state: Arc<AppState>) -> Result<()> {
        tracing::warn!(
            "No graphical UI is available in this build; use the CLI (--help) or the MCP server"
        );
        Ok(())
    }
}

pub use egui_app::run_ui;
