//! Ports (traits) : une couture par composant externe. Un composant absent
//! renvoie `Unavailable`/`NotWired`, jamais un « succès par défaut ».

use crate::collections::Collection;
use serde::Deserialize;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error, Clone, PartialEq)]
pub enum PortError {
    #[error("indisponible : {0}")]
    Unavailable(String),
    #[error("non câblé : {0}")]
    NotWired(String),
    #[error("refusé par le service : {0}")]
    Refused(String),
    #[error("protocole : {0}")]
    Protocol(String),
    #[error("délai dépassé ({0} s)")]
    Timeout(u64),
    #[error("E/S : {0}")]
    Io(String),
}

impl PortError {
    pub fn is_absent(&self) -> bool {
        matches!(self, PortError::Unavailable(_) | PortError::NotWired(_))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    Semantic,
    Lexical,
    #[default]
    Hybrid,
}

impl SearchMode {
    /// Méthode du pont MCP correspondante.
    pub fn method(self) -> &'static str {
        match self {
            SearchMode::Semantic => "search.semantic",
            SearchMode::Lexical => "search.keyword",
            SearchMode::Hybrid => "search.hybrid",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SearchMode::Semantic => "dense seul",
            SearchMode::Lexical => "BM25 seul",
            SearchMode::Hybrid => "hybride (BM25 + RRF)",
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct NoulVerdict {
    pub is_safe: bool,
    pub injection_score: f32,
    #[serde(default)]
    pub null_score: f32,
    #[serde(default)]
    pub is_ood: bool,
    #[serde(default)]
    pub ood_score: f32,
    #[serde(default)]
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChoiceVerdict {
    pub selected_index: usize,
    pub selected_option: String,
    pub probabilities: Vec<f32>,
    #[serde(default)]
    pub entropy: f32,
    pub confidence: f32,
    #[serde(default)]
    pub latency_ms: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ChunkScore {
    pub chunk_index: usize,
    #[serde(default)]
    pub text_preview: String,
    pub relevance: f32,
    #[serde(default)]
    pub is_relevant: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ScoreVerdict {
    pub scores: Vec<ChunkScore>,
    #[serde(default)]
    pub latency_ms: u64,
    #[serde(default)]
    pub chunks_evaluated: usize,
    #[serde(default)]
    pub chunks_relevant: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LayaHealth {
    pub status: String,
    pub encoder_loaded: bool,
    pub noul_loaded: bool,
    pub choice_loaded: bool,
    pub score_loaded: bool,
}

#[derive(Debug, Clone)]
pub struct Hit {
    pub file_path: String,
    pub text: String,
    /// Score de Maieutik : similarité normalisée `[0, 1]`, 1 = meilleur.
    /// Les scores de modes différents ne sont comparables qu'à mode égal.
    pub score: f32,
    pub heading_path: Option<String>,
    pub collection: Collection,
    pub relevance: Option<f32>,
}

impl Hit {
    pub fn new(file_path: String, text: String, score: f32, heading: Option<String>) -> Self {
        let collection = Collection::of(Path::new(&file_path));
        Self {
            file_path,
            text,
            score,
            heading_path: heading,
            collection,
            relevance: None,
        }
    }
}

pub trait System1Port {
    /// Vrai si les verdicts viennent d'un moteur simulé (aucune protection réelle).
    fn is_mock(&self) -> bool;
    fn noul(&mut self, text: &str) -> Result<NoulVerdict, PortError>;
    fn choice(&mut self, context: &str, options: &[String]) -> Result<ChoiceVerdict, PortError>;
    fn score(&mut self, query: &str, chunks: &[String]) -> Result<ScoreVerdict, PortError>;
}

pub trait SearchPort {
    fn search(&mut self, query: &str, k: usize) -> Result<Vec<Hit>, PortError>;
}

pub trait AgentPort {
    fn submit(&mut self, task: &str) -> Result<String, PortError>;
}

pub trait GeneratorPort {
    fn answer(&mut self, question: &str, hits: &[Hit]) -> Result<String, PortError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_mode_maps_to_the_bridge_methods() {
        assert_eq!(SearchMode::Semantic.method(), "search.semantic");
        assert_eq!(SearchMode::Lexical.method(), "search.keyword");
        assert_eq!(SearchMode::Hybrid.method(), "search.hybrid");
        assert_eq!(SearchMode::default(), SearchMode::Hybrid);
    }
}
