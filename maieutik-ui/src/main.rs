//! Point d'entrée GUI. Arguments :
//!   --demo                 tout simulé, marqué comme tel
//!   --laya-bin CHEMIN      binaire `maieutik-mcp` (ou $LAYA_MCP_BIN)
//!   --laya-real            demande LAYA_MOCK=0 au serveur (défaut : simulé)
//!   --laya-dir DOSSIER     répertoire de travail du serveur (modèles relatifs)
//!   --socket CHEMIN        socket Maieutik (défaut : même résolution que le serveur)
//!   --mode MODE            recherche : semantic | keyword | hybrid (défaut : hybrid)
//!   --odysseus DOSSIER     racine Odysseus
//!   --llm-url URL          endpoint LLM (loopback uniquement)

use maieutik_ui::app::App;
use maieutik_ui::backend::{BackendConfig, LayaSetup};
use maieutik_ui::mcp_client::Launch;
use std::path::PathBuf;

fn parse() -> Result<BackendConfig, String> {
    let mut cfg = BackendConfig::default();
    let mut laya_bin = std::env::var_os("LAYA_MCP_BIN").map(PathBuf::from);
    let mut laya_real = false;
    let mut laya_dir: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        let mut val = |name: &str| args.next().ok_or_else(|| format!("{name} attend une valeur"));
        match a.as_str() {
            "--demo" => cfg.demo = true,
            "--laya-real" => laya_real = true,
            "--laya-bin" => laya_bin = Some(PathBuf::from(val("--laya-bin")?)),
            "--laya-dir" => laya_dir = Some(PathBuf::from(val("--laya-dir")?)),
            "--socket" => cfg.daemon_socket = Some(PathBuf::from(val("--socket")?)),
            "--mode" => {
                let raw = val("--mode")?;
                cfg.pipeline.search_mode = match raw.as_str() {
                    "semantic" | "dense" => maieutik_ui::ports::SearchMode::Semantic,
                    "keyword" | "bm25" | "lexical" => maieutik_ui::ports::SearchMode::Lexical,
                    "hybrid" => maieutik_ui::ports::SearchMode::Hybrid,
                    other => return Err(format!("--mode inconnu : {other} (semantic|keyword|hybrid)")),
                };
            }
            "--odysseus" => cfg.odysseus_root = Some(PathBuf::from(val("--odysseus")?)),
            "--llm-url" => cfg.llm_endpoint = Some(val("--llm-url")?),
            "-h" | "--help" => {
                return Err(include_str!("main.rs").lines().take(9).map(|l| l.trim_start_matches("//! ")).collect::<Vec<_>>().join("\n"))
            }
            other => return Err(format!("argument inconnu : {other}")),
        }
    }
    if let Some(program) = laya_bin {
        cfg.laya = Some(LayaSetup { launch: Launch { program, workdir: laya_dir, ..Launch::default() }, mock: !laya_real });
    }
    Ok(cfg)
}

fn main() -> eframe::Result {
    let cfg = match parse() {
        Ok(c) => c,
        Err(m) => {
            eprintln!("{m}");
            std::process::exit(2);
        }
    };
    if let Err(e) = cfg.validate() {
        eprintln!("configuration refusée : {e}");
        std::process::exit(2);
    }
    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_app_id("maieutik-ui"), // règle de fenêtre Hyprland / Omarchy
        ..Default::default()
    };
    eframe::run_native(
        "Maieutik",
        options,
        Box::new(move |cc| {
            let app = App::new(cc.egui_ctx.clone(), cfg).map_err(|e| e.to_string())?;
            Ok(Box::new(app))
        }),
    )
}
