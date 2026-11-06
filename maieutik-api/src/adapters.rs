//! Adaptateurs : implémentations concrètes des ports.
//!
//! Chaque adaptateur relie l'infrastructure (Base de données, API externe, etc.)
//! au domaine métier via les traits définis dans `ports.rs`.

use crate::ports::{AppError, FileCatalog, Generator};
use async_trait::async_trait;
use maieutik::core::indexer::VectorIndex;
use std::sync::Arc;

/// Adaptateur qui expose la liste des fichiers indexés via VectorIndex.
#[derive(Clone)]
pub struct VectorIndexCatalog {
    pub index: Arc<VectorIndex>,
}

#[async_trait]
impl FileCatalog for VectorIndexCatalog {
    async fn list(&self) -> Result<Vec<String>, AppError> {
        self.index
            .stored_file_paths()
            .map_err(|e| AppError::Index(e.to_string()))
    }
}

/// Adaptateur qui délègue les appels de chat à Ollama.
#[derive(Clone)]
pub struct OllamaGenerator {
    pub client: Arc<maieutik::llm::ollama::OllamaClient>,
}

#[async_trait]
impl Generator for OllamaGenerator {
    async fn chat(&self, messages: Vec<crate::ports::ChatMessage>) -> Result<String, AppError> {
        let ollama_messages: Vec<maieutik::llm::ollama::ChatMessage> = messages
            .into_iter()
            .map(|m| maieutik::llm::ollama::ChatMessage {
                role: m.role,
                content: m.content,
            })
            .collect();

        self.client
            .chat(ollama_messages)
            .await
            .map_err(|e| AppError::Generation(e.to_string()))
    }
}