//! Ports (traits) defining the application boundaries.
//!
//! Ces traits sont l'interface entre le domaine métier (maieutik-core)
//! et les adaptateurs (maieutik-api, maieutik-ui, etc.). Le principe
//! SOLID-DIP garantit que le code métier ne dépend pas des détails
//! techniques d'infrastructure.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

/// Message d'échange pour le chat.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
}

/// Error type used across the API adaptors.
#[derive(Debug, Clone)]
pub enum AppError {
    Config(String),
    Index(String),
    Generation(String),
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::Config(e) => write!(f, "Configuration error: {}", e),
            AppError::Index(e) => write!(f, "Index error: {}", e),
            AppError::Generation(e) => write!(f, "Generation error: {}", e),
        }
    }
}

impl std::error::Error for AppError {}

/// Port pour la liste des fichiers indexés.
///
/// Implémenté par l'adaptateur qui expose `vector_index` et par le service
/// `maieutik::core::scan`.
#[async_trait]
pub trait FileCatalog: Send + Sync + Clone {
    async fn list(&self) -> Result<Vec<String>, AppError>;
}

/// Port pour la génération de texte via un modèle LLM.
///
/// Implémenté par l'adaptateur qui délègue à `maieutik::llm::ollama::OllamaClient`.
#[async_trait]
pub trait Generator: Send + Sync + Clone {
    async fn chat(&self, messages: Vec<ChatMessage>) -> Result<String, AppError>;
}