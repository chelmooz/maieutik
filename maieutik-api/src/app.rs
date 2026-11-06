//! Application state — agrégateur des adaptateurs.
//!
//! L'API expose les ports métier (traits définis dans `ports.rs`).
//! Les adaptateurs concrets sont construits au démarrage.

use crate::adapters::{OllamaGenerator, VectorIndexCatalog};
use maieutik::AppState as MaieutikAppState;
use std::sync::Arc;

/// État API partagé entre les handlers axum.
#[derive(Clone)]
pub struct ApiState {
    /// Adaptateur pour la liste des fichiers.
    pub file_catalog: VectorIndexCatalog,
    /// Adaptateur pour le chat/génération (Option car non toujours disponible).
    pub generator: Option<OllamaGenerator>,
}

impl ApiState {
    /// Crée un nouvel état API à partir de l'état maieutik fourni.
    ///
    /// Le générateur est créé si `MAIEUTIK_LLM_MODEL` commence par `ollama://`.
    pub fn new(maieutik: Arc<MaieutikAppState>) -> Self {
        let index = maieutik
            .vector_index
            .as_ref()
            .expect("vector_index must be initialized before api starts")
            .clone();

        let file_catalog = VectorIndexCatalog { index };

        let generator = Self::build_generator();

        Self {
            file_catalog,
            generator,
        }
    }

    /// Construit le générateur Ollama si disponible.
    fn build_generator() -> Option<OllamaGenerator> {
        let model_config = std::env::var("MAIEUTIK_LLM_MODEL").ok()?;
        let model_name = model_config.strip_prefix("ollama://")?;

        let client =
            maieutik::llm::ollama::OllamaClient::new("http://localhost:11434", model_name).ok()?;

        Some(OllamaGenerator {
            client: Arc::new(client),
        })
    }
}
