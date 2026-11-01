//! Primitive Choice — Routeur d'Intention & Outils (Closed-Set Classification)
//!
//! Prend un contexte + liste d'options fermées → distribution de probabilités calibrée.
//! Sortie : `argmax` + probabilités par option (JSON valide garanti, zéro erreur de syntaxe).
//!
//! Cas d'usage :
//! - Router vers workspace Maieutik : ["contracts", "infra", "code", "general"]
//! - Choisir outil MCP : ["maieutik.search", "files.read", "bash.exec", "web.search"]
//! - Décider action : ["rag", "agent", "direct_answer", "clarify"]

use super::{LayaConfig, LayaError, Result, Encoder, EncoderOutput};
use ndarray::Array2;
use ort::session::{Session, SessionInputValue};
use ort::value::Tensor;
use std::sync::Arc;
use std::path::Path;
use tracing::{info, warn};

#[derive(Debug, Clone, serde::Deserialize, serde::Serialize, schemars::JsonSchema)]
pub struct ChoiceConfig {
    pub model_path: String,
    pub intra_threads: usize,
    pub fp16: bool,
    /// Température pour calibration softmax (1.0 = standard, <1 = plus sharp).
    #[serde(default = "default_temp")]
    pub temperature: f32,
    /// Nombre max d'options supportées.
    #[serde(default = "default_max_options")]
    pub max_options: usize,
    /// Environnement ONNX Runtime partagé.
    #[serde(skip)]
    pub environment: Option<Arc<ort::environment::Environment>>,
}

fn default_temp() -> f32 { 1.0 }
fn default_max_options() -> usize { 16 }

impl From<&LayaConfig> for ChoiceConfig {
    fn from(c: &LayaConfig) -> Self {
        Self {
            model_path: c.model_path.replace("modernbert-base.onnx", "choice-head.onnx"),
            intra_threads: c.intra_threads,
            fp16: c.fp16,
            temperature: default_temp(),
            max_options: default_max_options(),
            environment: None,
        }
    }
}

/// Sortie de la primitive Choice.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
pub struct ChoiceOutput {
    /// Option sélectionnée (indice dans la liste fournie).
    pub selected_index: usize,
    /// Nom de l'option sélectionnée.
    pub selected_option: String,
    /// Probabilités calibrées pour chaque option (somme = 1.0).
    pub probabilities: Vec<f32>,
    /// Entropie de la distribution (mesure d'incertitude).
    pub entropy: f32,
    /// Confiance = 1 - entropy / log(n_options).
    pub confidence: f32,
    /// Latence inférence ms.
    pub latency_ms: u64,
    /// Embedding CLS utilisé.
    pub cls_embedding: Vec<f32>,
}

impl ChoiceOutput {
    pub fn new(
        logits: &[f32],
        options: &[String],
        cls: Vec<f32>,
        latency_ms: u64,
        temperature: f32,
    ) -> Self {
        let probs = softmax_with_temp(logits, temperature);
        let selected_index = probs.iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);
        let selected_option = options.get(selected_index).cloned().unwrap_or_default();
        let entropy = probs.iter().map(|p| if *p > 0.0 { -p * p.ln() } else { 0.0 }).sum();
        let max_entropy = (options.len() as f32).ln();
        let confidence = if max_entropy > 0.0 { 1.0 - entropy / max_entropy } else { 1.0 };

        Self {
            selected_index,
            selected_option,
            probabilities: probs,
            entropy,
            confidence,
            latency_ms,
            cls_embedding: cls,
        }
    }
}

/// Tête Choice : classification multi-classe sur CLS embedding.
pub struct ChoiceHead {
    session: Option<Session>,
    config: ChoiceConfig,
    encoder: Arc<Encoder>,
}

impl ChoiceHead {
    /// Check if the model session is loaded.
    pub fn is_loaded(&self) -> bool {
        self.session.is_some()
    }

    pub async fn load(config: &LayaConfig, encoder: Arc<Encoder>) -> Result<Self> {
        let choice_cfg = ChoiceConfig::from(config);
        let session = if Path::new(&choice_cfg.model_path).exists() {
            info!("Chargement tête Choice depuis {}", choice_cfg.model_path);
            Some(Self::create_session(&choice_cfg)?)
        } else {
            warn!("Tête Choice introuvable ({}), utilisation fallback heuristique", choice_cfg.model_path);
            None
        };
        Ok(Self { session, config: choice_cfg, encoder })
    }

    /// Charge avec une config pré-remplie (incluant l'environnement).
    pub async fn load_with_config(config: ChoiceConfig, encoder: Arc<Encoder>) -> Result<Self> {
        let session = if Path::new(&config.model_path).exists() {
            info!("Chargement tête Choice depuis {}", config.model_path);
            Some(Self::create_session(&config)?)
        } else {
            warn!("Tête Choice introuvable ({}), utilisation fallback heuristique", config.model_path);
            None
        };
        Ok(Self { session, config, encoder })
    }

    fn create_session(cfg: &ChoiceConfig) -> Result<ort::session::Session> {
        let env = cfg.environment.as_ref().ok_or_else(|| LayaError::EnvironmentNotInitialized)?;
        Ok(ort::session::Session::builder(env)?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .with_intra_threads(cfg.intra_threads)?
            .with_optimized_model_path(&cfg.model_path)?
            .commit_from_file(&cfg.model_path)?)
    }

    /// Prédiction Choice : contexte + options fermées.
    pub async fn predict(&mut self, context: &str, options: &[String]) -> Result<ChoiceOutput> {
        if options.is_empty() {
            // Return a default choice instead of error
            let latency_ms = 1;
            return Ok(ChoiceOutput {
                selected_index: 0,
                selected_option: String::new(),
                probabilities: vec![],
                entropy: 0.0,
                confidence: 0.0,
                latency_ms,
                cls_embedding: vec![],
            });
        }
        
        // Truncate options if too many
        let options = if options.len() > self.config.max_options {
            &options[..self.config.max_options]
        } else {
            options
        };

        let start = std::time::Instant::now();

        // Construire l'entrée : "[CLS] contexte [SEP] option_1 [SEP] option_2 ..."
        let mut input_text = context.to_string();
        for opt in options {
            input_text.push_str(" [SEP] ");
            input_text.push_str(opt);
        }

        let enc_out = self.encoder.encode(&[&input_text]).await?;
        let cls = &enc_out.cls_embeddings[0];

        let logits = if let Some(mut session) = self.session.take() {
            let result = self.predict_onnx(&mut session, cls, options.len()).await;
            self.session = Some(session);
            result?
        } else {
            self.predict_heuristic(context, options)
        };

        let latency_ms = (start.elapsed().as_millis() as u64).max(1);
        Ok(ChoiceOutput::new(&logits, options, cls.to_vec(), latency_ms, self.config.temperature))
    }

    async fn predict_onnx(&self, session: &mut Session, cls: &[f32], n_options: usize) -> Result<Vec<f32>> {
        // Get output names before running (session.run takes &mut self)
        let output_infos = session.outputs();
        let output_names: Vec<String> = output_infos.iter().map(|o| o.name().to_string()).collect();

        let input = Array2::from_shape_vec((1, cls.len()), cls.to_vec())?;
        let tensor = Tensor::from_array(input)?;
        let inputs: Vec<(String, ort::value::Value)> = vec![("cls_embedding".to_string(), tensor.into())];
        let outputs = session.run(inputs)?;

        for (i, output) in outputs.into_iter().enumerate() {
            if let Ok((shape, data)) = output.1.try_extract_tensor::<f32>() {
                let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
                if dims == [1, n_options] || dims == [n_options] {
                    return Ok(data.to_vec());
                }
                // Fallback: si une seule sortie et elle a la bonne taille
                if output_names.len() == 1 && i == 0 && (dims == [1, n_options] || dims == [n_options]) {
                    return Ok(data.to_vec());
                }
            }
        }
        Err(LayaError::DimensionMismatch { expected: n_options, got: 0 })
    }

    /// Fallback heuristique : routage basé sur mots-clés dans le contexte.
    fn predict_heuristic(&self, context: &str, options: &[String]) -> Vec<f32> {
        let ctx = context.to_lowercase();
        let n_options = options.len();
        let mut logits = vec![0.0f32; n_options];

        // Mots-clés -> option cible
        let keyword_boosts = [
            ("contrat", "contracts"),
            ("bail", "contracts"),
            ("kubernetes", "infra"),
            ("déploiement", "infra"),
            ("deploiement", "infra"),
            ("rust", "code"),
            ("fonction", "code"),
            ("cherche", "maieutik.search"),
            ("notes", "maieutik.search"),
            ("commande", "bash.exec"),
            ("ls ", "bash.exec"),
            ("docs", "rag"),
            ("api", "rag"),
            ("analyse", "agent"),
            ("améliorations", "agent"),
            ("ameliorations", "agent"),
        ];

        // Boost logits pour les options qui matchent les mots-clés
        for (i, option) in options.iter().enumerate() {
            let opt_lower = option.to_lowercase();
            for (keyword, target) in &keyword_boosts {
                if ctx.contains(keyword) && opt_lower == *target {
                    logits[i] += 10.0; // Strong boost for exact match
                }
            }
        }

        // Si aucun mot-clé ne match, boost léger sur "general" ou dernière option
        if logits.iter().all(|&l| l == 0.0) {
            if let Some(idx) = options.iter().position(|o| o == "general") {
                logits[idx] = 1.0;
            } else if !logits.is_empty() {
                logits[n_options - 1] = 1.0;
            }
        }

        logits
    }
}

fn softmax_with_temp(logits: &[f32], temp: f32) -> Vec<f32> {
    let max_logit = logits.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let exp: Vec<f32> = logits.iter().map(|&x| ((x - max_logit) / temp).exp()).collect();
    let sum: f32 = exp.iter().sum();
    exp.into_iter().map(|x| x / sum).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn choice_output_probabilities_sum_to_one() {
        let options = vec!["a".into(), "b".into(), "c".into()];
        let logits = vec![2.0, 1.0, 0.0];
        let out = ChoiceOutput::new(&logits, &options, vec![], 0, 1.0);
        let sum: f32 = out.probabilities.iter().sum();
        assert!((sum - 1.0).abs() < 1e-5);
    }

    #[test]
    fn choice_confidence_bounds() {
        let options = vec!["a".into(), "b".into()];
        let out = ChoiceOutput::new(&[10.0, -10.0], &options, vec![], 0, 1.0);
        assert!(out.confidence > 0.9);
        assert_eq!(out.selected_option, "a");
    }

    #[test]
    fn softmax_temperature() {
        let logits = vec![1.0, 0.0];
        let p1 = softmax_with_temp(&logits, 1.0);
        let p_sharp = softmax_with_temp(&logits, 0.1);
        assert!(p_sharp[0] > p1[0]); // plus sharp = plus confiant
    }
}

impl Default for ChoiceConfig {
    fn default() -> Self {
        Self {
            model_path: "models/choice-head.onnx".into(),
            intra_threads: 4,
            fp16: true,
            temperature: default_temp(),
            max_options: default_max_options(),
            environment: None,
        }
    }
}
