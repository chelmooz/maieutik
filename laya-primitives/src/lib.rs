//! Laya System 1 Primitives
//!
//! ModernBERT-based non-generative encoder with three task-specific heads:
//! - **Noul**: Guardrail (safety, OOD, injection detection)
//! - **Choice**: Intent & tool router (closed-set classification)
//! - **Score**: RAG critic (relevance regression [0,1])
//!
//! All heads share the same frozen ModernBERT backbone (Apache 2.0).
//! Inference runs on CPU/NPU via ONNX Runtime (FP16).

pub mod encoder;
pub mod noul;
pub mod choice;
pub mod score;
pub mod batch;
pub mod metrics;

pub use encoder::{Encoder, EncoderConfig, EncoderOutput, ModernBertEncoder, MockEncoder};
pub use noul::{NoulHead, NoulConfig, NoulOutput};
pub use choice::{ChoiceHead, ChoiceConfig, ChoiceOutput};
pub use score::{ScoreHead, ScoreConfig, ScoreOutput};
pub use batch::{BatchInference, BatchConfig, BatchRequest, BatchResponse};
pub use metrics::{PrimitiveMetrics, MetricsSnapshot};

use std::sync::Arc;
use tokio::sync::RwLock;
use thiserror::Error;

/// Erreurs unifiées pour toutes les primitives System 1.
#[derive(Error, Debug, Clone)]
pub enum LayaError {
    #[error("Modèle non chargé: {0}")]
    ModelNotLoaded(String),
    #[error("Erreur ONNX Runtime: {0}")]
    Ort(String),
    #[error("Erreur tokenisation: {0}")]
    Tokenizer(String),
    #[error("Dimension inattendue: attendu {expected}, reçu {got}")]
    DimensionMismatch { expected: usize, got: usize },
    #[error("Batch trop grand: max {max}, reçu {got}")]
    BatchTooLarge { max: usize, got: usize },
    #[error("Configuration invalide: {0}")]
    InvalidConfig(String),
    #[error("Entrée invalide: {0}")]
    InvalidInput(String),
    #[error("Environnement ONNX non initialisé")]
    EnvironmentNotInitialized,
}

impl<T> From<ort::Error<T>> for LayaError {
    fn from(e: ort::Error<T>) -> Self {
        LayaError::Ort(e.to_string())
    }
}

impl From<ndarray::ShapeError> for LayaError {
    fn from(e: ndarray::ShapeError) -> Self {
        LayaError::InvalidConfig(e.to_string())
    }
}

use std::sync::OnceLock;

static ONNX_ENV: OnceLock<Arc<ort::environment::Environment>> = OnceLock::new();

/// Retourne l'environnement ONNX Runtime partagé (singleton par processus).
pub fn get_ort_environment() -> Result<Arc<ort::environment::Environment>> {
    let env = ONNX_ENV.get_or_init(|| {
        ort::environment::init()
            .with_name("laya-system")
            .build()
            .map(Arc::new)
            .expect("Failed to initialize ONNX Runtime environment")
    });
    Ok(Arc::clone(env))
}

pub type Result<T> = std::result::Result<T, LayaError>;

/// Configuration globale partagée par toutes les primitives.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct LayaConfig {
    /// Chemin vers le modèle ModernBERT ONNX (encodeur backbone).
    pub model_path: String,
    /// Chemin vers le tokenizer (tokenizer.json).
    pub tokenizer_path: String,
    /// Device ONNX: "cpu", "cuda", "directml", "coreml".
    #[serde(default = "default_device")]
    pub device: String,
    /// Nombre de threads intra-op pour ONNX Runtime.
    #[serde(default = "default_threads")]
    pub intra_threads: usize,
    /// Longueur max de séquence (ModernBERT: 8192).
    #[serde(default = "default_max_seq_len")]
    pub max_seq_len: usize,
    /// Taille de batch max pour l'inférence groupée.
    #[serde(default = "default_max_batch")]
    pub max_batch_size: usize,
    /// Utiliser FP16 (half precision) si supporté.
    #[serde(default = "default_fp16")]
    pub fp16: bool,
    /// Si true, utilise un encodeur mock (déterministe) pour les tests sans modèle.
    #[serde(default)]
    pub mock: bool,
}

fn default_device() -> String { "cpu".into() }
fn default_threads() -> usize { 4 }
fn default_max_seq_len() -> usize { 8192 }
fn default_max_batch() -> usize { 64 }
fn default_fp16() -> bool { true }

impl Default for LayaConfig {
    fn default() -> Self {
        // Utiliser mock par défaut pour éviter les problèmes ONNX Runtime en dev/test
        // En production, définir mock=false et fournir les vrais modèles
        let mock = std::env::var("LAYA_MOCK").map(|v| v == "1" || v == "true").unwrap_or(true);
        Self {
            model_path: "models/modernbert-base.onnx".into(),
            tokenizer_path: "models/tokenizer.json".into(),
            device: default_device(),
            intra_threads: default_threads(),
            max_seq_len: default_max_seq_len(),
            max_batch_size: default_max_batch(),
            fp16: default_fp16(),
            mock,
        }
    }
}

/// État partagé : encodeur + têtes chargées une seule fois.
#[derive(Clone)]
pub struct LayaSystem {
    pub encoder: Arc<Encoder>,
    pub noul: Arc<RwLock<NoulHead>>,
    pub choice: Arc<RwLock<ChoiceHead>>,
    pub score: Arc<RwLock<ScoreHead>>,
    pub batch: Arc<BatchInference>,
    pub metrics: Arc<PrimitiveMetrics>,
}

impl LayaSystem {
    /// Charge le système complet depuis la config.
    pub async fn load(config: LayaConfig) -> Result<Self> {
        // 1. Récupérer l'environnement ONNX singleton
        let environment = get_ort_environment()?;

        // 2. Préparer les configs avec l'environnement injecté
        let mut encoder_cfg = EncoderConfig::from(&config);
        encoder_cfg.environment = Some(environment.clone());

        let mut noul_cfg = NoulConfig::from(&config);
        noul_cfg.environment = Some(environment.clone());

        let mut choice_cfg = ChoiceConfig::from(&config);
        choice_cfg.environment = Some(environment.clone());

        let mut score_cfg = ScoreConfig::from(&config);
        score_cfg.environment = Some(environment.clone());

        // 3. Charger l'encodeur (réel ou mock) avec l'environnement
        let encoder = Arc::new(Encoder::load(encoder_cfg).await?);

        // 4. Charger les têtes avec les configs enrichies
        let noul = Arc::new(RwLock::new(NoulHead::load_with_config(noul_cfg, encoder.clone()).await?));
        let choice = Arc::new(RwLock::new(ChoiceHead::load_with_config(choice_cfg, encoder.clone()).await?));
        let score = Arc::new(RwLock::new(ScoreHead::load_with_config(score_cfg, encoder.clone()).await?));

        // 5. Créer le batch inference (sans workers pour l'instant)
        let batch = Arc::new(BatchInference::new(config.max_batch_size));
        let metrics = Arc::new(PrimitiveMetrics::new());

        Ok(Self {
            encoder,
            noul,
            choice,
            score,
            batch,
            metrics,
        })
    }

    /// Inférence Noul (guardrail) — single.
    pub async fn noul(&self, text: &str) -> Result<NoulOutput> {
        let _timer = self.metrics.noul_latency.start_timer();
        let output = self.noul.write().await.predict(text).await?;
        self.metrics.noul_total.inc();
        Ok(output)
    }

    /// Inférence Choice (router) — single.
    pub async fn choice(&self, context: &str, options: &[String]) -> Result<ChoiceOutput> {
        let _timer = self.metrics.choice_latency.start_timer();
        let output = self.choice.write().await.predict(context, options).await?;
        self.metrics.choice_total.inc();
        Ok(output)
    }

    /// Inférence Score (RAG critic) — single.
    pub async fn score(&self, query: &str, chunks: &[String]) -> Result<ScoreOutput> {
        let _timer = self.metrics.score_latency.start_timer();
        let output = self.score.write().await.predict(query, chunks).await?;
        self.metrics.score_total.inc();
        Ok(output)
    }

    /// Inférence groupée (batch) pour les trois primitives.
    pub async fn batch(&self, req: BatchRequest) -> Result<BatchResponse> {
        self.batch.process(req, self).await
    }

    /// Snapshot métriques Prometheus.
    pub fn metrics_snapshot(&self) -> MetricsSnapshot {
        self.metrics.snapshot()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_defaults_are_sane() {
        let cfg = LayaConfig::default();
        assert_eq!(cfg.max_seq_len, 8192);
        assert_eq!(cfg.max_batch_size, 64);
        assert!(cfg.fp16);
    }
}