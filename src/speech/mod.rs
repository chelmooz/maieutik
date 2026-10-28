//! Speech recognition & VAD — silero-vad via ONNX Runtime.

use anyhow::Result;
use ort::session::Session;
use ort::value::Tensor;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::info;

#[derive(Debug, Clone)]
pub struct SpeechModelPaths {
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub joiner: PathBuf,
    pub tokens: PathBuf,
    pub vad_model: PathBuf,
}

impl Default for SpeechModelPaths {
    fn default() -> Self {
        let base = crate::config::store::models_dir()
            .expect("Impossible de déterminer le répertoire des modèles : home directory introuvable");
        Self {
            encoder: base.join("parakeet-encoder.fp16.onnx"),
            decoder: base.join("parakeet-decoder.int8.onnx"),
            joiner: base.join("parakeet-joiner.int8.onnx"),
            tokens: base.join("parakeet-tokens.txt"),
            vad_model: base.join("silero_vad.onnx"),
        }
    }
}

impl SpeechModelPaths {
    pub fn all_exist(&self) -> bool {
        self.encoder.exists()
            && self.decoder.exists()
            && self.joiner.exists()
            && self.tokens.exists()
            && self.vad_model.exists()
    }
}

/// Silero VAD wrapper using ort directly
pub struct SileroVad {
    session: Session,
    h: ndarray::Array2<f32>,
    c: ndarray::Array2<f32>,
    sample_rate: i64,
    threshold: f32,
}

impl SileroVad {
    pub fn new(model_path: &Path, threshold: f32) -> Result<Self> {
        // Shared process-wide ORT environment (see core::ort_env). The VAD is
        // tiny, one intra-op thread is plenty.
        let session = crate::core::ort_env::build_session(model_path, 1)?;

        let h = ndarray::Array2::zeros((1, 128));
        let c = ndarray::Array2::zeros((1, 128));
        let sample_rate = 16000i64;

        info!("Initialized silero VAD via ort");
        Ok(Self {
            session,
            h,
            c,
            sample_rate,
            threshold,
        })
    }

    pub fn process(&mut self, samples: &[i16]) -> Result<f32> {
        let f32_samples: Vec<f32> = samples.iter().map(|s| *s as f32 / 32768.0).collect();
        let input = ndarray::Array2::from_shape_vec((1, f32_samples.len()), f32_samples)?;

        let sr_array = ndarray::array![self.sample_rate];
        let h_array = self.h.clone();
        let c_array = self.c.clone();

        let outputs = self.session.run(ort::inputs![
            "input" => Tensor::from_array(input)?,
            "sample_rate" => Tensor::from_array(sr_array)?,
            "h" => Tensor::from_array(h_array)?,
            "c" => Tensor::from_array(c_array)?,
        ])?;

        // rc.15: `try_extract_tensor` returns `(&Shape, &[T])`.
        let (sp_shape, sp_data) = outputs[0].try_extract_tensor::<f32>()?;
        let (h_shape, h_data) = outputs[1].try_extract_tensor::<f32>()?;
        let (c_shape, c_data) = outputs[2].try_extract_tensor::<f32>()?;

        let h_dims: Vec<usize> = h_shape.iter().map(|d| *d as usize).collect();
        let c_dims: Vec<usize> = c_shape.iter().map(|d| *d as usize).collect();
        self.h = ndarray::Array2::from_shape_vec((h_dims[0], h_dims[1]), h_data.to_vec())?;
        self.c = ndarray::Array2::from_shape_vec((c_dims[0], c_dims[1]), c_data.to_vec())?;

        if sp_shape.num_elements() == 0 || sp_data.is_empty() {
            anyhow::bail!("Silero VAD returned an empty speech probability tensor");
        }
        Ok(sp_data[0])
    }

    pub fn is_speech(&mut self, samples: &[i16]) -> Result<bool> {
        let prob = self.process(samples)?;
        Ok(prob > self.threshold)
    }
}

/// High-level speech service (stub — sherpa-onnx integration pending)
pub struct SpeechService {
    vad: Arc<Mutex<SileroVad>>,
}

impl SpeechService {
    pub async fn new() -> Result<Self> {
        let paths = SpeechModelPaths::default();
        if !paths.all_exist() {
            return Err(anyhow::anyhow!(
                "Speech models not found. Run model download first."
            ));
        }
        let vad = SileroVad::new(&paths.vad_model, 0.5)?;
        info!("Initialized silero VAD via ort");
        Ok(Self {
            vad: Arc::new(Mutex::new(vad)),
        })
    }

    pub async fn start_session(&self) -> Result<SpeechSession> {
        Ok(SpeechSession { vad: self.vad.clone() })
    }
}

pub struct SpeechSession {
    vad: Arc<Mutex<SileroVad>>,
}

impl SpeechSession {
    pub async fn vad_check(&self, samples: &[i16]) -> Result<bool> {
        let mut vad = self.vad.lock().await;
        vad.is_speech(samples)
    }
}

/// Meeting transcription controller (stub)
pub mod meeting {
    use anyhow::Result;
    use crate::llm::ChatCompletionProvider;
    use crate::speech::SpeechService;
    use std::path::Path;
    use std::sync::Arc;
    use tracing::info;

    pub struct MeetingController {
        // Réservés au branchement de la transcription de réunion (stub) :
        // conservés pour que l'API de `new` reste celle prévue, pas encore lus.
        #[allow(dead_code)]
        speech: Arc<SpeechService>,
        #[allow(dead_code)]
        llm: Arc<dyn ChatCompletionProvider>,
        recording: bool,
    }

    impl MeetingController {
        pub fn new(speech: Arc<SpeechService>, llm: Arc<dyn ChatCompletionProvider>) -> Self {
            Self {
                speech,
                llm,
                recording: false,
            }
        }

        pub async fn start_meeting(&mut self, output_path: &Path) -> Result<()> {
            info!("Starting meeting recording: {}", output_path.display());
            self.recording = true;
            Ok(())
        }

        pub async fn stop_meeting(&mut self) -> Result<String> {
            self.recording = false;
            info!("Meeting stopped");
            Ok("Meeting transcript placeholder".to_string())
        }
    }
}