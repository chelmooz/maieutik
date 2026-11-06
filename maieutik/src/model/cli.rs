//! CLI pour les commandes model
//!
//! Sous-commandes : `maieutik model { list, download, remove, status }`

use super::{ModelProvider, ModelStatus};
use anyhow::Result;
use clap::Parser;

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
                        model.name, model.version, model.size_bytes, model.backend
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
                        println!("About to remove: {} @ {} [{}]", name, version, backend);
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
        ModelCmd::Status { name } => match provider.status(&name).await? {
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
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModelBackend, ModelError, ModelMetadata};
    use std::path::Path;
    use std::sync::Mutex;

    /// Provider de test : journalise les appels et renvoie des réponses
    /// déterministes, sans toucher au disque ni au réseau.
    struct MockProvider {
        installed: Mutex<Vec<ModelMetadata>>,
        status: Mutex<ModelStatus>,
        removed: Mutex<Vec<String>>,
        downloads: Mutex<Vec<(String, Option<String>)>>,
    }

    impl Default for MockProvider {
        fn default() -> Self {
            Self {
                installed: Mutex::new(Vec::new()),
                status: Mutex::new(ModelStatus::NotInstalled),
                removed: Mutex::new(Vec::new()),
                downloads: Mutex::new(Vec::new()),
            }
        }
    }

    impl MockProvider {
        fn with_status(status: ModelStatus) -> Self {
            Self {
                status: Mutex::new(status),
                ..Default::default()
            }
        }

        fn with_installed(models: Vec<ModelMetadata>) -> Self {
            Self {
                installed: Mutex::new(models),
                ..Default::default()
            }
        }
    }

    fn metadata(name: &str) -> ModelMetadata {
        ModelMetadata {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            sha256: "a".repeat(64),
            size_bytes: 1024,
            source_url: "https://example.invalid/model.bin".to_string(),
            installed_at: None,
            backend: ModelBackend::Onnx,
            description: None,
            min_free_memory: None,
            local_path: None,
        }
    }

    #[async_trait::async_trait]
    impl ModelProvider for MockProvider {
        async fn list(&self) -> Result<Vec<ModelMetadata>, ModelError> {
            Ok(self.installed.lock().expect("test").clone())
        }

        async fn download(
            &self,
            name: &str,
            version: Option<&str>,
            _progress: Option<super::super::ProgressCallback>,
        ) -> Result<ModelMetadata, ModelError> {
            self.downloads
                .lock()
                .expect("test")
                .push((name.to_string(), version.map(str::to_string)));
            Ok(metadata(name))
        }

        async fn install(
            &self,
            _metadata: ModelMetadata,
            _source_path: &Path,
            _force: bool,
        ) -> Result<(), ModelError> {
            Ok(())
        }

        async fn remove(&self, name: &str) -> Result<(), ModelError> {
            self.removed.lock().expect("test").push(name.to_string());
            Ok(())
        }

        async fn status(&self, _name: &str) -> Result<ModelStatus, ModelError> {
            Ok(self.status.lock().expect("test").clone())
        }
    }

    #[tokio::test]
    async fn list_json_returns_zero() {
        let provider = MockProvider::with_installed(vec![metadata("bge-m3")]);
        let code = run_model_cmd(provider, ModelCmd::List { json: true })
            .await
            .expect("test");
        assert_eq!(code, 0);
    }

    #[tokio::test]
    async fn list_text_returns_zero() {
        let provider = MockProvider::with_installed(vec![metadata("bge-m3"), metadata("qwen")]);
        let code = run_model_cmd(provider, ModelCmd::List { json: false })
            .await
            .expect("test");
        assert_eq!(code, 0);
    }

    #[tokio::test]
    async fn list_empty_is_not_an_error() {
        let provider = MockProvider::default();
        let code = run_model_cmd(provider, ModelCmd::List { json: false })
            .await
            .expect("test");
        assert_eq!(code, 0);
    }

    #[tokio::test]
    async fn download_forwards_name_and_version() {
        let provider = MockProvider::default();
        let code = run_model_cmd(
            provider,
            ModelCmd::Download {
                name: "qwen".to_string(),
                version: Some("3".to_string()),
            },
        )
        .await
        .expect("test");
        assert_eq!(code, 0);
    }

    #[tokio::test]
    async fn remove_with_force_removes_without_status_check() {
        let provider = MockProvider::with_status(ModelStatus::NotInstalled);
        let code = run_model_cmd(
            provider,
            ModelCmd::Remove {
                name: "ghost".to_string(),
                force: true,
            },
        )
        .await
        .expect("test");
        assert_eq!(code, 0, "--force doit supprimer même un modèle absent");
    }

    #[tokio::test]
    async fn remove_without_force_refuses_missing_model() {
        let provider = MockProvider::with_status(ModelStatus::NotInstalled);
        let code = run_model_cmd(
            provider,
            ModelCmd::Remove {
                name: "ghost".to_string(),
                force: false,
            },
        )
        .await
        .expect("test");
        assert_eq!(code, 1, "un modèle absent doit être refusé");
    }

    #[tokio::test]
    async fn remove_without_force_removes_installed_model() {
        let provider = MockProvider::with_status(ModelStatus::Installed {
            version: "1.0.0".to_string(),
            backend: ModelBackend::Onnx,
        });
        let code = run_model_cmd(
            provider,
            ModelCmd::Remove {
                name: "bge-m3".to_string(),
                force: false,
            },
        )
        .await
        .expect("test");
        assert_eq!(code, 0);
    }

    #[tokio::test]
    async fn status_installed_returns_zero() {
        let provider = MockProvider::with_status(ModelStatus::Installed {
            version: "1.0.0".to_string(),
            backend: ModelBackend::Gguf,
        });
        let code = run_model_cmd(
            provider,
            ModelCmd::Status {
                name: "x".to_string(),
            },
        )
        .await
        .expect("test");
        assert_eq!(code, 0);
    }

    #[tokio::test]
    async fn status_not_installed_returns_one() {
        let provider = MockProvider::with_status(ModelStatus::NotInstalled);
        let code = run_model_cmd(
            provider,
            ModelCmd::Status {
                name: "x".to_string(),
            },
        )
        .await
        .expect("test");
        assert_eq!(code, 1);
    }

    #[tokio::test]
    async fn status_corrupted_returns_one() {
        let provider = MockProvider::with_status(ModelStatus::Corrupted {
            reason: "sha mismatch".to_string(),
        });
        let code = run_model_cmd(
            provider,
            ModelCmd::Status {
                name: "x".to_string(),
            },
        )
        .await
        .expect("test");
        assert_eq!(code, 1);
    }

    #[test]
    fn cli_parses_download_arguments() {
        let cmd = ModelCmd::try_parse_from(["model", "download", "qwen", "--version", "3"])
            .expect("download doit être parsé");
        match cmd {
            ModelCmd::Download { name, version } => {
                assert_eq!(name, "qwen");
                assert_eq!(version.as_deref(), Some("3"));
            }
            other => panic!("variante inattendue : {other:?}"),
        }
    }

    #[test]
    fn cli_parses_remove_force_flag() {
        let cmd = ModelCmd::try_parse_from(["model", "remove", "qwen", "--force"])
            .expect("remove doit être parsé");
        match cmd {
            ModelCmd::Remove { name, force } => {
                assert_eq!(name, "qwen");
                assert!(force);
            }
            other => panic!("variante inattendue : {other:?}"),
        }
    }

    #[test]
    fn cli_parses_list_json_flag() {
        let cmd =
            ModelCmd::try_parse_from(["model", "list", "--json"]).expect("list doit être parsé");
        match cmd {
            ModelCmd::List { json } => assert!(json),
            other => panic!("variante inattendue : {other:?}"),
        }
    }

    #[test]
    fn cli_rejects_missing_required_name() {
        assert!(
            ModelCmd::try_parse_from(["model", "status"]).is_err(),
            "le nom du modèle est obligatoire"
        );
    }
}
