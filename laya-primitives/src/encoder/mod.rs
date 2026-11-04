//! ModernBERT Encoder (ONNX Runtime)
//!
//! Charge un modèle ModernBERT exporté en ONNX (backbone encodeur uniquement).
//! Produit des représentations contextuelles dense (CLS pooling ou mean pooling).
//!
//! Modèles supportés :
//! - `answerdotai/ModernBERT-base` (300M params, 8192 seq len)
//! - `answerdotai/ModernBERT-large` (400M params, 8192 seq len)

use super::{LayaConfig, LayaError, Result};
use ndarray;
use ort::session::Session;
use ort::value::Tensor;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokenizers::Tokenizer;
use tracing::{info, warn};

/// Configuration spécifique à l'encodeur.
#[derive(Debug, Clone, serde::Deserialize, serde::Serialize)]
pub struct EncoderConfig {
    pub model_path: String,
    pub tokenizer_path: String,
    pub intra_threads: usize,
    pub max_seq_len: usize,
    pub max_batch_size: usize,
    pub fp16: bool,
    /// Si true, utilise un encodeur mock (déterministe) pour les tests sans modèle.
    #[serde(default)]
    pub mock: bool,
    /// Environnement ONNX Runtime partagé.
    #[serde(skip)]
    pub environment: Option<Arc<ort::environment::Environment>>,
}

impl From<&LayaConfig> for EncoderConfig {
    fn from(c: &LayaConfig) -> Self {
        Self {
            model_path: c.model_path.clone(),
            tokenizer_path: c.tokenizer_path.clone(),
            intra_threads: c.intra_threads,
            max_seq_len: c.max_seq_len,
            max_batch_size: c.max_batch_size,
            fp16: c.fp16,
            mock: c.mock,
            environment: None, // Sera injecté par LayaSystem
        }
    }
}

/// Sortie de l'encodeur : embeddings + métadonnées.
#[derive(Debug, Clone)]
pub struct EncoderOutput {
    /// Embeddings CLS : [batch_size, hidden_dim]
    pub cls_embeddings: Vec<Vec<f32>>,
    /// Embeddings mean-pooled : [batch_size, hidden_dim]
    pub mean_embeddings: Vec<Vec<f32>>,
    /// Longueurs réelles des séquences (sans padding).
    pub seq_lens: Vec<usize>,
    /// Hidden dimension (768 pour base, 1024 pour large).
    pub hidden_dim: usize,
    /// Batch size traité.
    pub batch_size: usize,
}

impl EncoderOutput {
    pub fn new(cls: Vec<Vec<f32>>, mean: Vec<Vec<f32>>, lens: Vec<usize>) -> Self {
        let hidden_dim = cls.first().map(|v| v.len()).unwrap_or(0);
        let batch_size = cls.len();
        Self {
            cls_embeddings: cls,
            mean_embeddings: mean,
            seq_lens: lens,
            hidden_dim,
            batch_size,
        }
    }
}

/// Encodeur ModernBERT via ONNX Runtime.
pub struct ModernBertEncoder {
    session: Mutex<ort::session::Session>,
    tokenizer: Tokenizer,
    config: EncoderConfig,
    input_names: Vec<String>,
    #[allow(dead_code)]
    output_names: Vec<String>,
    hidden_dim: usize,
}

impl ModernBertEncoder {
    /// Charge le modèle et le tokenizer, crée la session ONNX.
    pub async fn load(config: &LayaConfig) -> Result<Self> {
        let encoder_cfg = EncoderConfig::from(config);
        Self::load_with_config(encoder_cfg).await
    }

    /// Charge avec une config complète (incluant l'environnement).
    pub async fn load_with_config(config: EncoderConfig) -> Result<Self> {
        let environment = config
            .environment
            .as_ref()
            .ok_or(LayaError::EnvironmentNotInitialized)?
            .clone();
        info!("Chargement ModernBERT depuis {}", config.model_path);

        // 1. Tokenizer
        let mut tokenizer = Tokenizer::from_file(&config.tokenizer_path)
            .map_err(|e| LayaError::Tokenizer(e.to_string()))?;

        // Truncation à max_seq_len (ModernBERT n'a pas de truncation par défaut)
        tokenizer
            .with_truncation(Some(tokenizers::TruncationParams {
                strategy: tokenizers::TruncationStrategy::LongestFirst,
                stride: 0,
                max_length: config.max_seq_len,
                direction: tokenizers::TruncationDirection::Right,
            }))
            .map_err(|e| LayaError::InvalidConfig(e.to_string()))?;

        // 2. Session ONNX
        let model_path = Path::new(&config.model_path);
        if !model_path.exists() {
            return Err(LayaError::ModelNotLoaded(format!(
                "Modèle introuvable: {}. Téléchargez modernbert-base.onnx depuis HuggingFace (answerdotai/ModernBERT-base) et exportez en ONNX.",
                model_path.display()
            )));
        }

        let mut session = ort::session::Session::builder(&environment)?
            .with_optimization_level(ort::session::builder::GraphOptimizationLevel::Level3)?
            .with_intra_threads(config.intra_threads)?
            .with_optimized_model_path(model_path)?
            .commit_from_file(model_path)?;

        // Récupérer noms d'entrée/sortie
        let input_names: Vec<String> = session
            .inputs()
            .iter()
            .map(|i| i.name().to_string())
            .collect();
        let output_names: Vec<String> = session
            .outputs()
            .iter()
            .map(|o| o.name().to_string())
            .collect();

        info!("Inputs ONNX: {:?}", input_names);
        info!("Outputs ONNX: {:?}", output_names);

        // Déduire hidden_dim depuis la sortie (last_hidden_state: [batch, seq, hidden])
        let hidden_dim = Self::infer_hidden_dim(
            &mut session,
            &input_names,
            &output_names,
            config.max_seq_len,
        )?;

        Ok(Self {
            session: Mutex::new(session),
            tokenizer,
            config,
            input_names,
            output_names,
            hidden_dim,
        })
    }

    fn infer_hidden_dim(
        session: &mut Session,
        input_names: &[String],
        output_names: &[String],
        _max_seq_len: usize,
    ) -> Result<usize> {
        // Forward pass bidon pour inspecter la forme de sortie
        let dummy_ids = ndarray::Array2::from_elem((1, 2), 0i64); // [CLS] [SEP]
        let dummy_mask = ndarray::Array2::from_elem((1, 2), 1i64);
        let dummy_type = ndarray::Array2::from_elem((1, 2), 0i64);

        let mut inputs: Vec<(String, ort::value::Value)> = Vec::new();
        for name in input_names {
            let val = match name.as_str() {
                "input_ids" => Tensor::from_array(dummy_ids.clone())?,
                "attention_mask" => Tensor::from_array(dummy_mask.clone())?,
                "token_type_ids" => Tensor::from_array(dummy_type.clone())?,
                _ => continue,
            };
            inputs.push((name.clone(), val.into()));
        }

        let outputs = session.run(inputs)?;
        // Chercher last_hidden_state ou sequence_output
        for (name, output) in output_names.iter().zip(outputs.iter()) {
            if name.contains("hidden") || name.contains("sequence") || name.contains("last") {
                if let Ok((shape, _)) = output.1.try_extract_tensor::<f32>() {
                    let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
                    // [batch, seq, hidden]
                    if dims.len() == 3 {
                        return Ok(dims[2]);
                    }
                }
            }
        }
        // Fallback: ModernBERT-base = 768, large = 1024
        warn!("Impossible de déduire hidden_dim, fallback 768");
        Ok(768)
    }

    /// Encode un batch de textes → embeddings CLS + mean-pooled.
    pub async fn encode(&self, texts: &[&str]) -> Result<EncoderOutput> {
        if texts.is_empty() {
            return Err(LayaError::InvalidInput("Batch vide".into()));
        }
        if texts.len() > self.config.max_batch_size {
            return Err(LayaError::BatchTooLarge {
                max: self.config.max_batch_size,
                got: texts.len(),
            });
        }

        // 1. Tokenisation
        let mut all_ids = Vec::new();
        let mut all_mask = Vec::new();
        let mut all_type_ids = Vec::new();
        let mut seq_lens = Vec::new();

        for text in texts {
            let encoding = self
                .tokenizer
                .encode(*text, true)
                .map_err(|e| LayaError::Tokenizer(e.to_string()))?;

            let ids: Vec<i64> = encoding.get_ids().iter().map(|&v| v as i64).collect();
            let mask: Vec<i64> = encoding
                .get_attention_mask()
                .iter()
                .map(|&v| v as i64)
                .collect();
            let type_ids: Vec<i64> = encoding.get_type_ids().iter().map(|&v| v as i64).collect();

            seq_lens.push(ids.len());
            all_ids.push(ids);
            all_mask.push(mask);
            all_type_ids.push(type_ids);
        }

        // 2. Padding à la longueur max du batch
        let max_len = seq_lens
            .iter()
            .max()
            .copied()
            .unwrap_or(1)
            .min(self.config.max_seq_len);
        let batch_size = texts.len();

        let mut padded_ids = ndarray::Array2::zeros((batch_size, max_len));
        let mut padded_mask = ndarray::Array2::zeros((batch_size, max_len));
        let mut padded_type = ndarray::Array2::zeros((batch_size, max_len));

        for (i, ((ids, mask), types)) in all_ids
            .into_iter()
            .zip(all_mask)
            .zip(all_type_ids)
            .enumerate()
        {
            let len = ids.len().min(max_len);
            padded_ids
                .slice_mut(ndarray::s![i, ..len])
                .assign(&ndarray::Array1::from_vec(ids[..len].to_vec()));
            padded_mask
                .slice_mut(ndarray::s![i, ..len])
                .assign(&ndarray::Array1::from_vec(mask[..len].to_vec()));
            padded_type
                .slice_mut(ndarray::s![i, ..len])
                .assign(&ndarray::Array1::from_vec(types[..len].to_vec()));
        }

        // 3. Inférence ONNX
        let mut inputs: Vec<(String, ort::value::Value)> = Vec::new();
        for name in &self.input_names {
            let val = match name.as_str() {
                "input_ids" => Tensor::from_array(padded_ids.clone())?,
                "attention_mask" => Tensor::from_array(padded_mask.clone())?,
                "token_type_ids" => Tensor::from_array(padded_type.clone())?,
                _ => continue,
            };
            inputs.push((name.clone(), val.into()));
        }

        let mut session_guard = self.session.lock().unwrap();
        let outputs = session_guard.run(inputs)?;

        // 4. Extraire last_hidden_state [batch, seq, hidden]
        let hidden = outputs
            .iter()
            .find(|(name, _)| {
                name.contains("hidden") || name.contains("sequence") || name.contains("last")
            })
            .ok_or(LayaError::InvalidConfig(
                "Sortie last_hidden_state introuvable".into(),
            ))?
            .1;

        let (shape, data) = hidden.try_extract_tensor::<f32>()?;
        let dims: Vec<usize> = shape.iter().map(|d| *d as usize).collect();
        // dims = [batch, seq, hidden]
        if dims.len() != 3 {
            return Err(LayaError::DimensionMismatch {
                expected: 3,
                got: dims.len(),
            });
        }
        let [_b, seq, hidden_dim] = [dims[0], dims[1], dims[2]];

        // 5. CLS pooling (token 0) + Mean pooling (sur tokens non-padding)
        let mut cls_embeddings = Vec::with_capacity(batch_size);
        let mut mean_embeddings = Vec::with_capacity(batch_size);

        for (i, &seq_len) in seq_lens.iter().enumerate().take(batch_size) {
            let actual_len = seq_len.min(seq);
            let start = i * seq * hidden_dim;

            // CLS = premier token
            let cls_start = start;
            let cls_end = cls_start + hidden_dim;
            let cls_vec = data[cls_start..cls_end].to_vec();
            cls_embeddings.push(cls_vec);

            // Mean pooling sur tokens valides (mask == 1)
            let mut mean = vec![0.0f32; hidden_dim];
            let mut count = 0;
            for s in 0..actual_len {
                let token_start = start + s * hidden_dim;
                let token_end = token_start + hidden_dim;
                for (j, &val) in data[token_start..token_end].iter().enumerate() {
                    mean[j] += val;
                }
                count += 1;
            }
            if count > 0 {
                for v in &mut mean {
                    *v /= count as f32;
                }
            }
            mean_embeddings.push(mean);
        }

        Ok(EncoderOutput::new(
            cls_embeddings,
            mean_embeddings,
            seq_lens,
        ))
    }

    /// Dimension cachée du modèle.
    pub fn hidden_dim(&self) -> usize {
        self.hidden_dim
    }

    /// Longueur max de séquence.
    pub fn max_seq_len(&self) -> usize {
        self.config.max_seq_len
    }
}

impl std::fmt::Debug for ModernBertEncoder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ModernBertEncoder")
            .field("hidden_dim", &self.hidden_dim)
            .field("max_seq_len", &self.config.max_seq_len)
            .field("intra_threads", &self.config.intra_threads)
            .finish()
    }
}

/// Encodeur Mock pour les tests sans modèle réel.
/// Retourne des embeddings déterministes basés sur un hash du texte.
#[derive(Debug)]
pub struct MockEncoder {
    config: EncoderConfig,
    hidden_dim: usize,
}

impl MockEncoder {
    pub fn new(config: EncoderConfig) -> Self {
        Self {
            hidden_dim: config.max_seq_len.min(768), // 768 pour ModernBERT-base
            config,
        }
    }

    pub async fn encode(&self, texts: &[&str]) -> Result<EncoderOutput> {
        if texts.is_empty() {
            return Err(LayaError::InvalidInput("Batch vide".into()));
        }
        if texts.len() > self.config.max_batch_size {
            return Err(LayaError::BatchTooLarge {
                max: self.config.max_batch_size,
                got: texts.len(),
            });
        }

        let batch_size = texts.len();
        let hidden_dim = self.hidden_dim;

        let mut cls_embeddings = Vec::with_capacity(batch_size);
        let mut mean_embeddings = Vec::with_capacity(batch_size);
        let mut seq_lens = Vec::with_capacity(batch_size);

        for text in texts {
            // Embedding déterministe basé sur un hash simple du texte
            let mut hash: u64 = 0;
            for b in text.bytes() {
                hash = hash.wrapping_mul(31).wrapping_add(b as u64);
            }

            let mut cls = vec![0.0f32; hidden_dim];
            let mut mean = vec![0.0f32; hidden_dim];

            if !text.trim().is_empty() {
                for i in 0..hidden_dim {
                    // Générer des valeurs déterministes basées sur le hash
                    let val =
                        ((hash.wrapping_mul(i as u64 + 1) as f32) / u64::MAX as f32) * 2.0 - 1.0;
                    cls[i] = val;
                    mean[i] = val * 0.5; // Mean slightly different
                }

                // Normaliser (L2)
                let cls_norm: f32 = cls.iter().map(|v| v * v).sum::<f32>().sqrt();
                let mean_norm: f32 = mean.iter().map(|v| v * v).sum::<f32>().sqrt();
                if cls_norm > 0.0 {
                    for v in &mut cls {
                        *v /= cls_norm;
                    }
                }
                if mean_norm > 0.0 {
                    for v in &mut mean {
                        *v /= mean_norm;
                    }
                }
            }

            cls_embeddings.push(cls);
            mean_embeddings.push(mean);
            seq_lens.push(text.len().min(self.config.max_seq_len));
        }

        Ok(EncoderOutput::new(
            cls_embeddings,
            mean_embeddings,
            seq_lens,
        ))
    }

    pub fn hidden_dim(&self) -> usize {
        self.hidden_dim
    }
    pub fn max_seq_len(&self) -> usize {
        self.config.max_seq_len
    }
}

/// Encodeur unifié : soit ModernBERT réel, soit Mock pour les tests.
#[derive(Debug)]
pub enum Encoder {
    Real(Box<ModernBertEncoder>),
    Mock(MockEncoder),
}

impl Encoder {
    pub async fn load(config: EncoderConfig) -> Result<Self> {
        if config.mock {
            info!("Chargement encodeur MOCK (tests)");
            Ok(Self::Mock(MockEncoder::new(config)))
        } else {
            info!("Chargement ModernBERT depuis {}", config.model_path);
            let config_clone = config.clone();
            match ModernBertEncoder::load_with_config(config).await {
                Ok(encoder) => Ok(Self::Real(Box::new(encoder))),
                Err(e) => {
                    warn!(
                        "Échec chargement ModernBERT ({}), fallback MOCK: {}",
                        config_clone.model_path, e
                    );
                    let mut mock_config = config_clone;
                    mock_config.mock = true;
                    Ok(Self::Mock(MockEncoder::new(mock_config)))
                }
            }
        }
    }

    pub async fn encode(&self, texts: &[&str]) -> Result<EncoderOutput> {
        match self {
            Self::Real(e) => e.encode(texts).await,
            Self::Mock(e) => e.encode(texts).await,
        }
    }

    pub fn hidden_dim(&self) -> usize {
        match self {
            Self::Real(e) => e.hidden_dim(),
            Self::Mock(e) => e.hidden_dim(),
        }
    }

    pub fn max_seq_len(&self) -> usize {
        match self {
            Self::Real(e) => e.max_seq_len(),
            Self::Mock(e) => e.max_seq_len(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore = "nécessite modèle ONNX"]
    async fn test_load_encoder() {
        let config = LayaConfig {
            model_path: "models/modernbert-base.onnx".into(),
            tokenizer_path: "models/tokenizer.json".into(),
            ..Default::default()
        };
        let encoder = ModernBertEncoder::load(&config).await;
        assert!(encoder.is_ok());
    }
}
