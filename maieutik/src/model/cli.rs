//! CLI pour les commandes model
//!
//! Sous-commandes : `maieutik model { list, download, remove, status }`

use super::{ModelProvider, ModelMetadata, ModelStatus};
use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Debug, Parser)]
pub enum ModelCmd {
    /// Liste les modèles installés
    List {
        /// Sortie JSON
        #[arg(long)]
        json: bool,
    },
    /// Télécharge et installe un modèle
    Download {
        /// Nom du modèle
        name: String,
        /// Version spécifique
        #[arg(long)]
        version: Option<String>,
    },
    /// Supprime un modèle
    Remove {
        /// Nom du modèle
        name: String,
        /// Force la suppression
        #[arg(long)]
        force: bool,
    },
    /// Affiche le statut d'un modèle
    Status {
        /// Nom du modèle
        name: String,
    },
}

/// Exécute la commande model
pub async fn run_model_cmd<Provider: ModelProvider>(
    provider: Provider,
    cmd: ModelCmd,
) -> Result<i32> {
    match cmd {
        ModelCmd::List { json } => {
            let models = provider.list().await?;
            if json {
                println!("{}", serde_json::to_string_pretty(&models)?);
            } else {
                for model in models {
                    println!(
                        "{} {} ({} bytes) @ {}",
                        model.name,
                        model.version,
                        model.size_bytes,
                        model.backend
                    );
                }
            }
            Ok(0)
        }
        ModelCmd::Download { name, version } => {
            let metadata = provider.download(&name, version.as_deref(), None).await?;
            println!("Installed: {} @ {}", metadata.name, metadata.version);
            Ok(0)
        }
        ModelCmd::Remove { name, force } => {
            if force {
                provider.remove(&name).await?;
                println!("Removed: {}", name);
                Ok(0)
            } else {
                match provider.status(&name).await? {
                    ModelStatus::Installed { version, backend } => {
                        println!(
                            "About to remove: {} @ {} [{}]",
                            name, version, backend
                        );
                        // TODO: confirmation interactive
                        provider.remove(&name).await?;
                        println!("Removed: {}", name);
                        Ok(0)
                    }
                    _ => {
                        eprintln!("Model '{}' not installed", name);
                        Ok(1)
                    }
                }
            }
        }
        ModelCmd::Status { name } => {
            match provider.status(&name).await? {
                ModelStatus::Installed { version, backend } => {
                    println!("Installed: {} @ {} [{}]", name, version, backend);
                    Ok(0)
                }
                ModelStatus::NotInstalled => {
                    eprintln!("Model '{}' not installed", name);
                    Ok(1)
                }
                ModelStatus::Corrupted { reason } => {
                    eprintln!("Model '{}' corrupted: {}", name, reason);
                    Ok(1)
                }
            }
        }
    }
}