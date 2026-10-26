//! Maieutik — Main entry point
//!
//! Codes de sortie :
//!   0 succès complet
//!   1 échec partiel (fichier sans texte, corpus vide, dossier partiel, échec de fichier)
//!   2 usage invalide (clap)
//!   3 commande non implémentée (`chat`, `model`, `--mcp-stdio`)
//!   4 environnement indisponible (modèle, index, `Folders` vide, dossiers absents)
//! stdout ne porte que des résultats ; tous les diagnostics vont sur stderr.

use anyhow::Result;
use clap::{Parser, Subcommand};
use maieutik::config;
use maieutik::{run_watcher, AppState};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

const EXIT_NOT_IMPLEMENTED: i32 = 3;
const EXIT_UNAVAILABLE: i32 = 4;

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

// Les champs de Chat/Model ne sont plus lus tant que ces commandes
// sont non implémentées (code 3).
#[allow(dead_code)]
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
        /// Nombre maximal de résultats (>= 1)
        #[arg(short, long, default_value = "10", value_parser = parse_limit)]
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

#[allow(dead_code)]
#[derive(Debug, Subcommand)]
enum ModelAction {
    List,
    Download { name: String },
    Remove { name: String },
    Status,
}

/// `--limit` doit être un entier >= 1 (clap sort en code 2 sinon).
/// `value_parser!(usize).range(..)` n'existe pas dans clap 4 : parseur dédié.
fn parse_limit(raw: &str) -> Result<usize, String> {
    let n: usize = raw.parse().map_err(|e| format!("{e}"))?;
    if n == 0 {
        Err("doit être supérieur ou égal à 1".to_string())
    } else {
        Ok(n)
    }
}

/// Journaux sur stderr, couleurs seulement sur un terminal, bruit ORT filtré.
/// `with_log_level` d'ort est sans effet avec la feature `tracing` : le filtre
/// se pose ici, dans le subscriber.
fn init_logging(debug: bool) -> Result<()> {
    let default_filter = if debug { "debug,ort=warn" } else { "info,ort=warn" };
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_writer(std::io::stderr)
        .with_ansi(std::io::stderr().is_terminal())
        .with_target(false)
        .compact()
        .try_init()
        .map_err(|e| anyhow::anyhow!("{e}"))
}

/// Message sur stderr, stdout vide, code 3.
fn not_implemented(label: &str) -> i32 {
    eprintln!("Non implémenté : {label}.");
    EXIT_NOT_IMPLEMENTED
}

fn not_implemented_label(cmd: &Commands) -> Option<&'static str> {
    match cmd {
        Commands::Chat { .. } => Some("chat"),
        Commands::Model { .. } => Some("model"),
        _ => None,
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    init_logging(cli.debug)?;
    let code = run(cli).await?;
    if code != 0 {
        std::process::exit(code);
    }
    Ok(())
}

async fn run(cli: Cli) -> Result<i32> {
    // Commandes non implémentées : refus avant toute initialisation.
    if cli.mcp_stdio {
        return Ok(not_implemented("--mcp-stdio"));
    }
    if let Some(label) = cli.command.as_ref().and_then(not_implemented_label) {
        return Ok(not_implemented(label));
    }

    // Single instance check (cross-platform lock file)
    let lock = single_instance::SingleInstance::new("maieutik").ok();
    if lock.is_none() {
        info!("Another instance is already running");
        return Ok(0); // Exit silently
    }

    // Load settings
    let mut state = match AppState::new().await {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Environnement indisponible : {e:#}");
            return Ok(EXIT_UNAVAILABLE);
        }
    };
    info!("Maieutik starting...");

    // `Folders` est vérifié AVANT l'initialisation du modèle (coûteuse).
    if matches!(cli.command, Some(Commands::Index { .. }))
        && state.settings.read().await.folders.is_empty()
    {
        eprintln!("Aucun dossier à indexer : renseigne `Folders` dans config.json.");
        return Ok(EXIT_UNAVAILABLE);
    }

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

    Ok(0)
}

fn index_unavailable() -> i32 {
    eprintln!(
        "Index indisponible : modèle d'embedding introuvable ou illisible (voir les journaux)."
    );
    EXIT_UNAVAILABLE
}

async fn handle_command(cmd: Commands, state: Arc<AppState>) -> Result<i32> {
    match cmd {
        Commands::Index { force } => {
            let Some(idx) = &state.vector_index else {
                return Ok(index_unavailable());
            };
            let settings = state.settings.read().await.clone();
            info!("Indexing... (force={})", force);
            let report = maieutik::core::scan::index_all(idx, &settings, force).await?;
            for (path, msg) in &report.failed {
                eprintln!("échec : {} : {}", path.display(), msg);
            }
            for path in &report.no_text {
                eprintln!("sans texte : {}", path.display());
            }
            if report.is_empty_corpus() {
                eprintln!("Aucun fichier à indexer dans les dossiers configurés (extension ou taille hors limites).");
            }
            println!(
                "{} indexé(s), {} sans texte, {} échec(s)",
                report.indexed,
                report.no_text.len(),
                report.failed.len()
            );
            Ok(report.exit_code())
        }
        Commands::Search { query, limit } => {
            let Some(idx) = &state.vector_index else {
                return Ok(index_unavailable());
            };
            let results = idx.search(&query, limit).await?;
            for (path, text, score) in results {
                println!(
                    "{:.3} | {} | {}",
                    score,
                    path,
                    text.chars().take(100).collect::<String>()
                );
            }
            Ok(0)
        }
        // Déjà refusées dans `run` ; garde-fou si l'ordre changeait.
        Commands::Chat { .. } | Commands::Model { .. } => Ok(not_implemented("commande")),
    }
}
