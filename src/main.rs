//! Maieutik — Main entry point
//!
//! Codes de sortie :
//!   0 succès complet (ou arrêt propre demandé par un signal en `--watch`)
//!   1 échec partiel (fichier sans texte, corpus vide, dossier partiel, échec de fichier)
//!   2 usage invalide (clap, `--watch` combiné à une autre commande)
//!   3 commande non implémentée (`chat`, `model`, `--mcp-stdio`)
//!   4 environnement indisponible (modèle, index, `Folders` vide, dossiers absents)
//! stdout ne porte que des résultats ; tous les diagnostics vont sur stderr.

use anyhow::Result;
use clap::{Parser, Subcommand};
use maieutik::config;
use maieutik::{run_watcher, run_watcher_with_shutdown, AppState};
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use tracing::{info, warn};
use tracing_subscriber::EnvFilter;

const EXIT_USAGE: i32 = 2;
const EXIT_NOT_IMPLEMENTED: i32 = 3;
const EXIT_UNAVAILABLE: i32 = 4;

/// Nom du verrou d'instance unique, surchargeable par
/// `MAIEUTIK_INSTANCE_NAME`.
///
/// Les tests d'intégration s'en servent pour s'isoler : `single-instance`
/// 0.3 lie, sous Linux, un socket abstrait dont le nom est global au réseau
/// (et non plus un fichier relatif au dossier courant comme en 0.1), donc
/// deux bacs à sable qui partageraient le nom par défaut « maieutik » se
/// refuseraient mutuellement.
fn instance_name() -> String {
    std::env::var("MAIEUTIK_INSTANCE_NAME").unwrap_or_else(|_| "maieutik".to_owned())
}

#[derive(Parser)]
#[command(name = "maieutik", version, about = "Local-first notes with AI")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Run MCP server over stdio (for Claude Desktop, etc.)
    #[arg(long = "mcp-stdio")]
    mcp_stdio: bool,

    /// Watch folders and re-index on change, until SIGINT/SIGTERM
    #[arg(long = "watch")]
    watch: bool,

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
    // `--watch` est un mode : il ne se combine ni avec une sous-commande ni
    // avec le serveur MCP. Refus explicite avant toute initialisation.
    if cli.watch && (cli.command.is_some() || cli.mcp_stdio) {
        eprintln!("--watch ne se combine pas avec une sous-commande ni avec --mcp-stdio.");
        return Ok(EXIT_USAGE);
    }

    // Commandes non implémentées : refus avant toute initialisation.
    if cli.mcp_stdio {
        // Lance le serveur MCP sur stdio — ne retourne que si la connexion se ferme.
        let server = Arc::new(maieutik::mcp::McpServer::new());
        if let Err(e) = server.run_stdio().await { eprintln!("Erreur serveur MCP: {e}"); return Ok(EXIT_UNAVAILABLE); } return Ok(0);
    }
    if let Some(label) = cli.command.as_ref().and_then(not_implemented_label) {
        return Ok(not_implemented(label));
    }

    // Verrou d'instance unique. `SingleInstance::new` réussit même quand le
    // verrou est déjà détenu par un autre processus : seul `is_single()` dit
    // si ce processus est le propriétaire. Le verrou doit rester vivant
    // jusqu'à la fin de `run`, sinon il est libéré immédiatement.
    let instance = instance_name();
    let _lock = match single_instance::SingleInstance::new(&instance) {
        Ok(lock) => lock,
        Err(e) => {
            eprintln!("Verrou d'instance indisponible : {e}");
            return Ok(EXIT_UNAVAILABLE);
        }
    };
    if !_lock.is_single() {
        eprintln!("Une autre instance de Maieutik est déjà en cours d'exécution.");
        return Ok(EXIT_UNAVAILABLE);
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
    let model_path = config::store::embedder_model_path()?;
    let tokenizer_path = config::store::embedder_tokenizer_path()?;
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

    // Mode service explicite : le watcher tourne au premier plan, piloté par
    // le signal d'arrêt (voir `run_watcher_with_shutdown`).
    let state_arc = Arc::new(state);
    if cli.watch {
        return watch_mode(state_arc).await;
    }

    // Start file watcher in background
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

/// Mode service (`maieutik --watch`) : garde le processus vivant, réindexe à
/// chaque changement et sort en 0 sur SIGINT/SIGTERM après avoir vidé les
/// événements en attente — c'est le seul appelant de
/// `run_watcher_with_shutdown` dans le binaire.
///
/// Refus avant démarrage quand le service n'aurait rien à faire : sans
/// dossier, ou sans index (impossible de réindexer sans modèle).
async fn watch_mode(state: Arc<AppState>) -> Result<i32> {
    let folders = state.settings.read().await.folders.clone();
    if folders.is_empty() {
        eprintln!("Aucun dossier à surveiller : renseigne `Folders` dans config.json.");
        return Ok(EXIT_UNAVAILABLE);
    }
    if state.vector_index.is_none() {
        return Ok(index_unavailable());
    }

    // Réconciliation de démarrage : les fichiers disparus pendant que le
    // service était arrêté sont purgés avant de commencer à surveiller.
    if let Some(idx) = &state.vector_index {
        match maieutik::core::scan::reconcile_startup(idx).await {
            Ok(report) => {
                for path in &report.kept_unreadable {
                    warn!("Réconciliation : métadonnées illisibles, entrée conservée : {path}");
                }
                for path in &report.kept_unverified {
                    warn!(
                        "Réconciliation : empreinte inconnue, entrée conservée : {path} \
                         (réindexée au prochain 'maieutik index')"
                    );
                }
                for (path, err) in &report.failed {
                    warn!("Réconciliation : purge impossible {path} : {err}");
                }
                if !report.removed.is_empty() {
                    info!(
                        "Réconciliation : {} fichier(s) disparu(s) purgé(s) de l'index.",
                        report.removed.len()
                    );
                }
                if !report.reindexed.is_empty() {
                    info!(
                        "Réconciliation : {} fichier(s) modifié(s) pendant l'arrêt réindexé(s).",
                        report.reindexed.len()
                    );
                }
            }
            // Une réconciliation ratée est signalée mais n'empêche pas de
            // surveiller : c'est une amélioration, pas un prérequis.
            Err(e) => warn!("Réconciliation impossible : {e:#}"),
        }
    }

    info!(
        "Surveillance de {} dossier(s) ; SIGINT ou SIGTERM pour arrêter.",
        folders.len()
    );
    run_watcher_with_shutdown(state, shutdown_signal()).await?;
    info!("Arrêt propre terminé.");

    Ok(0)
}

/// Attend SIGINT ou SIGTERM. Si aucun gestionnaire ne peut être installé, le
/// futur ne se résout jamais : le signal garde son effet par défaut au lieu
/// de devenir un no-op silencieux.
async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{signal, SignalKind};

        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = term.recv() => info!("SIGTERM reçu, arrêt propre"),
                    res = tokio::signal::ctrl_c() => match res {
                        Ok(()) => info!("SIGINT reçu, arrêt propre"),
                        Err(e) => {
                            warn!("SIGINT indisponible : {e}");
                            std::future::pending::<()>().await;
                        }
                    },
                }
            }
            Err(e) => {
                warn!("SIGTERM indisponible : {e}");
                match tokio::signal::ctrl_c().await {
                    Ok(()) => info!("SIGINT reçu, arrêt propre"),
                    Err(e) => {
                        warn!("SIGINT indisponible : {e}");
                        std::future::pending::<()>().await;
                    }
                }
            }
        }
    }

    #[cfg(not(unix))]
    {
        match tokio::signal::ctrl_c().await {
            Ok(()) => info!("SIGINT reçu, arrêt propre"),
            Err(e) => {
                warn!("SIGINT indisponible : {e}");
                std::future::pending::<()>().await;
            }
        }
    }
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
