//! Maieutik — Main entry point

use anyhow::Result;
use clap::{Parser, Subcommand};
use maieutik::{AppState, run_watcher, save_settings_if_changed};
use maieutik::config;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, Level};
use tracing_subscriber::FmtSubscriber;

#[derive(Parser)]
#[command(name = "maieutik", version, about = "Local-first notes with AI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Run MCP server over stdio (for Claude Desktop, etc.)
    #[arg(long = "mcp-stdio")]
    mcp_stdio: bool,

    /// Start hidden in tray
    #[arg(long = "tray")]
    start_in_tray: bool,

    /// File to open on startup
    #[arg(long = "file")]
    file: Option<PathBuf>,

    /// Config directory override
    #[arg(long = "config-dir")]
    config_dir: Option<PathBuf>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Index watched folders
    Index {
        /// Force full re-index
        #[arg(long)]
        force: bool,
    },
    /// Search notes
    Search {
        query: String,
        #[arg(short, long, default_value = "10")]
        limit: usize,
    },
    /// Chat with notes
    Chat {
        message: String,
        #[arg(long)]
        file: Option<PathBuf>,
    },
    /// Manage models (download, list, remove)
    Model {
        #[command(subcommand)]
        action: ModelAction,
    },
}

#[derive(Debug, Subcommand)]
enum ModelAction {
    List,
    Download { name: String },
    Remove { name: String },
    Status,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging
    let level = if cli.debug { Level::DEBUG } else { Level::INFO };
    let subscriber = FmtSubscriber::builder()
        .with_max_level(level)
        .with_target(false)
        .compact()
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    // Single instance check (cross-platform lock file)
    let lock = single_instance::SingleInstance::new("maieutik").ok();
    if lock.is_none() {
        info!("Another instance is already running");
        if cli.mcp_stdio {
            // Try to connect to existing instance via named pipe
            return run_mcp_stdio_client().await;
        }
        return Ok(()); // Exit silently
    }

    if cli.mcp_stdio {
        return run_mcp_stdio_server().await;
    }

    // Load settings
    let mut state = AppState::new().await?;
    info!("Maieutik starting...");

    // Initialize embedder (ONNX model + matching XLM-RoBERTa tokenizer)
    let model_path = config::store::embedder_model_path();
    let tokenizer_path = config::store::embedder_tokenizer_path();
    if model_path.exists() && tokenizer_path.exists() {
        if let Err(e) = state
            .initialize_indexer(model_path.clone(), tokenizer_path.clone())
            .await
        {
            warn!("Failed to initialize vector index: {}", e);
        }
    } else {
        info!(
            "Embedding model/tokenizer not found ({} / {})",
            model_path.display(),
            tokenizer_path.display()
        );
    }

    // Start file watcher in background
    let state_arc = Arc::new(state);
    let watcher_state = state_arc.clone();
    tokio::spawn(async move {
        if let Err(e) = run_watcher(watcher_state).await {
            tracing::error!("Watcher error: {}", e);
        }
    });

    // Handle CLI commands
    if let Some(cmd) = cli.command {
        return handle_command(cmd, state_arc).await;
    }

    // Default: start UI (headless build logs a warning and exits)
    maieutik::ui::run_ui(state_arc).await?;

    Ok(())
}

async fn handle_command(cmd: Commands, state: Arc<AppState>) -> Result<()> {
    match cmd {
        Commands::Index { force } => {
            info!("Indexing... (force={})", force);
            // TODO: implement full index
        }
        Commands::Search { query, limit } => {
            if let Some(idx) = &state.vector_index {
                let results = idx.search(&query, limit).await?;
                for (path, text, score) in results {
                    println!("{:.3} | {} | {}", score, path, text.chars().take(100).collect::<String>());
                }
            } else {
                println!("Vector index not initialized");
            }
        }
        Commands::Chat { message, file } => {
            println!("Chat: {} (file: {:?})", message, file);
            // TODO: implement
        }
        Commands::Model { action } => {
            println!("Model: {:?}", action);
        }
    }
    Ok(())
}

async fn run_mcp_stdio_server() -> Result<()> {
    info!("Starting MCP server over stdio");
    // TODO: implement MCP stdio server using jsonrpsee
    Ok(())
}

async fn run_mcp_stdio_client() -> Result<()> {
    info!("Connecting to existing instance via MCP");
    // TODO: implement named pipe client to activate existing window
    Ok(())
}