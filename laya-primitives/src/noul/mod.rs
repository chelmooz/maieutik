//! Primitive Noul — Guardrail & Détection Hors-Périmètre
//!
//! Classification multi-tâche sur la sortie de l'encodeur ModernBERT :
//! - `is_safe` : binaire (safe / unsafe)
//! - `is_ood` : out-of-distribution (hors périmètre connu)
//! - `injection_score` : probabilité d'injection de prompt / commande destructive
//! - `null_score` : probabilité que l'entrée soit vide/sans sens
//!
//! Seuil par défaut : `is_safe = (injection_score < 0.5) && (null_score < 0.5) && !is_ood`

use super::{LayaConfig, LayaError, Result, Encoder, EncoderOutput};
use ndarray::Array2;
use ort::session::{Session, SessionInputValue};
use ort::value::Tensor;
use std::sync::Arc;
use std::path::Path;
use tracing::{info, warn};

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct NoulConfig {
    pub model_path: String,
    pub intra_threads: usize,
    pub fp16: bool,
    /// Seuil injection pour considérer unsafe.
    #[serde(default = "default_injection_thresh")]
    pub injection_threshold: f32,
    /// Seuil null/empty.
    #[serde(default = "default_null_thresh")]
    pub null_threshold: f32,
    /// Seuil OOD (distance mahalanobis ou entropy).
    #[serde(default = "default_ood_thresh")]
    pub ood_threshold: f32,
    /// Environnement ONNX Runtime partagé.
    #[serde(skip)]
    pub environment: Option<Arc<ort::environment::Environment>>,
}

fn default_injection_thresh() -> f32 { 0.5 }
fn default_null_thresh() -> f32 { 0.5 }
fn default_ood_thresh() -> f32 { 0.7 }

impl From<&LayaConfig> for NoulConfig {
    fn from(c: &LayaConfig) -> Self {
        Self {
            model_path: c.model_path.replace("modernbert-base.onnx", "noul-head.onnx"),
            intra_threads: c.intra_threads,
            fp16: c.fp16,
            injection_threshold: default_injection_thresh(),
            null_threshold: default_null_thresh(),
            ood_threshold: default_ood_thresh(),
            environment: None,
        }
    }
}

/// Sortie de la primitive Noul.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct NoulOutput {
    /// True si l'entrée est considérée sûre (pas d'injection, pas null, pas OOD).
    pub is_safe: bool,
    /// Score d'injection de prompt [0,1].
    pub injection_score: f32,
    /// Score null/empty [0,1].
    pub null_score: f32,
    /// True si out-of-distribution détecté.
    pub is_ood: bool,
    /// Score OOD [0,1] (entropy ou distance).
    pub ood_score: f32,
    /// Latence inférence ms.
    pub latency_ms: u64,
    /// Embedding CLS utilisé (pour debug/chaînage).
    pub cls_embedding: Vec<f32>,
}

impl NoulOutput {
    pub fn new(
        injection: f32,
        null: f32,
        ood: f32,
        cls: Vec<f32>,
        latency_ms: u64,
        cfg: &NoulConfig,
    ) -> Self {
        let is_safe = injection < cfg.injection_threshold
            && null < cfg.null_threshold
            && ood < cfg.ood_threshold;
        Self {
            is_safe,
            injection_score: injection,
            null_score: null,
            is_ood: ood >= cfg.ood_threshold,
            ood_score: ood,
            latency_ms,
            cls_embedding: cls,
        }
    }
}

/// Tête Noul : charge un modèle ONNX distinct (fine-tuned sur ModernBERT CLS).
pub struct NoulHead {
    session: Option<ort::session::Session>,
    config: NoulConfig,
    encoder: Arc<Encoder>,
}

impl NoulHead {
    pub fn is_loaded(&self) -> bool { self.session.is_some() }
    pub async fn load(config: &LayaConfig, encoder: Arc<Encoder>) -> Result<Self> {
        let noul_cfg = NoulConfig::from(config);
        let session = if Path::new(&noul_cfg.model_path).exists() {
            info!("Chargement tête Noul depuis {}", noul_cfg.model_path);
            Some(Self::create_session(&noul_cfg)?)
        } else {
            warn!("Tête Noul introuvable ({}), utilisation fallback heuristique", noul_cfg.model_path);
            None
        };
        Ok(Self { session, config: noul_cfg, encoder })
    }

    /// Charge avec une config pré-remplie (incluant l'environnement).
    pub async fn load_with_config(config: NoulConfig, encoder: Arc<Encoder>) -> Result<Self> {
        let session = if Path::new(&config.model_path).exists() {
            info!("Chargement tête Noul depuis {}", config.model_path);
            Some(Self::create_session(&config)?)
        } else {
            warn!("Tête Noul introuvable ({}), utilisation fallback heuristique", config.model_path);
            None
        };
        Ok(Self { session, config, encoder })
    }

    fn create_session(cfg: &NoulConfig) -> Result<ort::session::Session> {
        let env = cfg.environment.as_ref().ok_or_else(|| LayaError::EnvironmentNotInitialized)?;
        Ok(ort::session::Session::builder(env)?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .with_intra_threads(cfg.intra_threads)?
            .with_optimized_model_path(&cfg.model_path)?
            .commit_from_file(&cfg.model_path)?)
    }

    /// Prédiction Noul sur un texte unique.
    pub async fn predict(&mut self, text: &str) -> Result<NoulOutput> {
        let start = std::time::Instant::now();
        let enc_out = self.encoder.encode(&[text]).await?;
        let cls = &enc_out.cls_embeddings[0];

        let (injection, null, ood) = if let Some(mut session) = self.session.take() {
            let result = self.predict_onnx(&mut session, cls).await;
            self.session = Some(session);
            result?
        } else {
            self.predict_heuristic(cls)
        };

        Ok(NoulOutput::new(injection, null, ood, cls.to_vec(), (start.elapsed().as_millis() as u64).max(1), &self.config))
    }

    async fn predict_onnx(&self, session: &mut Session, cls: &[f32]) -> Result<(f32, f32, f32)> {
        // Get output names before running (session.run takes &mut self)
        let output_infos = session.outputs();
        let output_names: Vec<String> = output_infos.iter().map(|o| o.name().to_string()).collect();
        let mut injection = 0.5;
        let mut null = 0.5;
        let mut ood = 0.5;

        let input = Array2::from_shape_vec((1, cls.len()), cls.to_vec())?;
        let tensor = Tensor::from_array(input)?;
        let inputs: Vec<(String, ort::value::Value)> = vec![("cls_embedding".to_string(), tensor.into())];
        let outputs = session.run(inputs)?;

        for (i, output) in outputs.into_iter().enumerate() {
            if let Ok((_shape, data)) = output.1.try_extract_tensor::<f32>() {
                if i < output_names.len() {
                    let name = &output_names[i];
                    if name.contains("injection") { injection = sigmoid(data[0]); }
                    else if name.contains("null") { null = sigmoid(data[0]); }
                    else if name.contains("ood") { ood = sigmoid(data[0]); }
                }
            }
        }
        Ok((injection, null, ood))
    }

    /// Fallback heuristique sans tête fine-tuned (basé sur normes CLS + entropy).
    fn predict_heuristic(&self, cls: &[f32]) -> (f32, f32, f32) {
        // Norme L2 de l'embedding CLS (embeddings OOD ont souvent une norme anormale)
        let norm: f32 = cls.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mean_abs: f32 = cls.iter().map(|x| x.abs()).sum::<f32>() / cls.len() as f32;

        // Heuristiques simples (à calibrer)
        let injection = (mean_abs > 2.5) as u8 as f32 * 0.3 + (norm > 30.0) as u8 as f32 * 0.4;
        let null = (norm < 0.5) as u8 as f32 * 0.8;
        let ood = ((norm > 40.0) || (norm < 0.5)) as u8 as f32 * 0.6;

        (injection.min(1.0), null.min(1.0), ood.min(1.0))
    }
}

fn sigmoid(x: f32) -> f32 {
    1.0 / (1.0 + (-x).exp())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sigmoid_bounds() {
        assert!((sigmoid(0.0) - 0.5).abs() < 1e-6);
        assert!(sigmoid(10.0) > 0.999);
        assert!(sigmoid(-10.0) < 0.001);
    }

    #[test]
    fn noul_output_safety_logic() {
        let cfg = NoulConfig {
            injection_threshold: 0.5,
            null_threshold: 0.5,
            ood_threshold: 0.7,
            ..Default::default()
        };
        let out = NoulOutput::new(0.3, 0.2, 0.1, vec![], 0, &cfg);
        assert!(out.is_safe);

        let out = NoulOutput::new(0.8, 0.1, 0.1, vec![], 0, &cfg);
        assert!(!out.is_safe);
    }
}

impl Default for NoulConfig {
    fn default() -> Self {
        Self {
            model_path: "models/noul-head.onnx".into(),
            intra_threads: 4,
            fp16: true,
            injection_threshold: 0.5,
            null_threshold: 0.5,
            ood_threshold: 0.7,
            environment: None,
        }
    }
}