//! LLM Generation — Intégration llama.cpp
//!
//! Support GPU/CPU hybride avec streaming.
//!
//! Configuration :
//! - MAIEUTIK_LLM_MODEL : chemin vers le modèle GGUF
//! - MAIEUTIK_LLM_CTX_SIZE : taille du contexte (défaut: 2048)
//! - MAIEUTIK_LLM_N_GPU_LAYERS : nombre de couches GPU (défaut: auto)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

/// Paramètres de génération
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationParams {
    /// Prompt à générer
    pub prompt: String,
    /// Taille max du contexte
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctx_size: Option<usize>,
    /// Batch size pour inférence
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<usize>,
    /// Temperature (0.0 - 2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p nucleus sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Top-k sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
    /// Penalité de répétition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f32>,
    /// Nombre max de tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    /// Couches à exécuter sur GPU (-1 = auto)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_gpu_layers: Option<isize>,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            ctx_size: Some(2048),
            batch_size: Some(512),
            temperature: Some(0.7),
            top_p: Some(0.9),
            top_k: Some(40),
            repeat_penalty: Some(1.1),
            max_tokens: Some(512),
            n_gpu_layers: None,
        }
    }
}

/// Résultat d'une génération
#[derive(Debug, Clone)]
pub struct GenerationResult {
    /// Texte généré
    pub text: String,
    /// Nombre de tokens utilisés
    pub tokens: TokenUsage,
    /// Durée de génération
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt: usize,
    pub completion: usize,
    pub total: usize,
}

/// Callback pour streaming les tokens
pub type StreamCallback = dyn Fn(&str) + Send + Sync;

/// Gestionnaire de modèles LLM
pub struct LlamaManager {
    model_path: PathBuf,
    params: GenerationParams,
    ready: bool,
}

impl LlamaManager {
    /// Crée un nouveau gestionnaire LLM
    pub fn new(model_path: PathBuf) -> Result<Self> {
        if !model_path.exists() {
            anyhow::bail!("Modèle non trouvé: {}", model_path.display());
        }

        Ok(Self {
            model_path,
            params: GenerationParams::default(),
            ready: false,
        })
    }

    /// Initialise le modèle (chargement en mémoire)
    pub fn init(&mut self) -> Result<()> {
        info!("Initialisation du modèle: {:?}", self.model_path);
        // TODO: intégration réelle avec llama-cpp-2
        // Pour l'instant, on simule
        self.ready = true;
        Ok(())
    }

    /// Vérifie si le modèle est chargé
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Génère un texte
    pub fn generate(&mut self, params: GenerationParams) -> Result<GenerationResult> {
        if !self.ready {
            self.init()?;
        }

        let start = std::time::Instant::now();

        // Placeholder - l'implémentation réelle utilisera llama-cpp-2
        let response = self.simulate_generation(&params.prompt);

        let duration = start.elapsed().as_millis();

        Ok(GenerationResult {
            text: response?,
            tokens: TokenUsage::default(),
            duration_ms: duration,
        })
    }

    /// Génération simulée pour les tests
    fn simulate_generation(&self, prompt: &str) -> Result<String> {
        // Pour les tests, on retourne une réponse simulée
        // L'implémentation réelle utilisera llama-cpp-2
        warn!("Génération simulée - modèle LLM non chargé");
        Ok(format!(
            "Réponse simulée pour: {}... (modèle LLM non chargé, utilisez MAIEUTIK_LLM_MODEL pour configurer)",
            prompt.chars().take(50).collect::<String>()
        ))
    }
}

/// Factory pour créer le générateur
pub fn create_llm_generator() -> Result<Arc<Mutex<LlamaManager>>> {
    let model_path = std::env::var("MAIEUTIK_LLM_MODEL")
        .map(PathBuf::from)
        .ok().ok_or_else(|| {
            anyhow::anyhow!(
                "Variable MAIEUTIK_LLM_MODEL non définie. \
                 Configurez le chemin vers votre modèle GGUF."
            )
        })?;

    Ok(Arc::new(Mutex::new(LlamaManager::new(model_path)?)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generation_params_default() {
        let params = GenerationParams::default();
        assert_eq!(params.ctx_size, Some(2048));
        assert_eq!(params.temperature, Some(0.7));
        assert_eq!(params.max_tokens, Some(512));
    }

    #[test]
    fn test_generation_params_serde() {
        let params = GenerationParams {
            prompt: "Hello".to_string(),
            temperature: Some(0.8),
            max_tokens: Some(100),
            ..Default::default()
        };

        let json = serde_json::to_string(&params).unwrap();
        let deserialized: GenerationParams = serde_json::from_str(&json).unwrap();

        assert_eq!(params.prompt, deserialized.prompt);
        assert_eq!(params.temperature, deserialized.temperature);
    }
}