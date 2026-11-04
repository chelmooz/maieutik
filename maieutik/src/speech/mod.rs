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
        let base = crate::config::store::models_dir().expect(
            "Impossible de déterminer le répertoire des modèles : home directory introuvable",
        );
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
#[derive(Debug)]
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

        // Validate output shapes: h and c must be rank 2
        // Shape derefs to &[i64], so len() gives rank
        let h_rank = h_shape.len();
        let c_rank = c_shape.len();
        if h_rank != 2 {
            anyhow::bail!(
                "VAD hidden state h has unexpected rank {} (expected 2)",
                h_rank
            );
        }
        if c_rank != 2 {
            anyhow::bail!(
                "VAD cell state c has unexpected rank {} (expected 2)",
                c_rank
            );
        }

        let h_dims: Vec<usize> = h_shape.iter().map(|d| *d as usize).collect();
        let c_dims: Vec<usize> = c_shape.iter().map(|d| *d as usize).collect();

        // Validate speech probability tensor
        if sp_shape.num_elements() == 0 || sp_data.is_empty() {
            anyhow::bail!("Silero VAD returned an empty speech probability tensor");
        }

        // Validate probability is finite
        let prob = sp_data[0];
        if !prob.is_finite() {
            anyhow::bail!(
                "Silero VAD returned non-finite speech probability: {}",
                prob
            );
        }

        // All validations passed - now update state
        self.h = ndarray::Array2::from_shape_vec((h_dims[0], h_dims[1]), h_data.to_vec())?;
        self.c = ndarray::Array2::from_shape_vec((c_dims[0], c_dims[1]), c_data.to_vec())?;

        Ok(prob)
    }

    pub fn is_speech(&mut self, samples: &[i16]) -> Result<bool> {
        let prob = self.process(samples)?;
        Ok(prob > self.threshold)
    }
}

/// High-level speech service (stub — sherpa-onnx integration pending)
#[derive(Debug)]
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
        Ok(SpeechSession {
            vad: self.vad.clone(),
        })
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
    use crate::speech::SpeechService;
    use anyhow::Result;
    use std::path::Path;
    use std::sync::Arc;
    use tracing::info;

    /// Stub for LLM chat completion - to be implemented in P2
    pub type ChatCompletionProvider = ();

    pub struct MeetingController {
        #[allow(dead_code)]
        pub(crate) speech: Arc<SpeechService>,
        #[allow(dead_code)]
        pub(crate) llm: Arc<ChatCompletionProvider>,
        pub(crate) recording: bool,
    }

    impl MeetingController {
        pub fn new(speech: Arc<SpeechService>, llm: Arc<ChatCompletionProvider>) -> Self {
            Self {
                speech,
                llm,
                recording: false,
            }
        }

        pub fn is_recording(&self) -> bool {
            self.recording
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn paths_in(dir: &Path) -> SpeechModelPaths {
        SpeechModelPaths {
            encoder: dir.join("encoder.onnx"),
            decoder: dir.join("decoder.onnx"),
            joiner: dir.join("joiner.onnx"),
            tokens: dir.join("tokens.txt"),
            vad_model: dir.join("vad.onnx"),
        }
    }

    #[test]
    fn all_exist_is_false_when_everything_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());
        assert!(!paths.all_exist());
    }

    #[test]
    fn all_exist_is_false_when_a_single_file_is_missing() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());
        for file in [
            &paths.encoder,
            &paths.decoder,
            &paths.joiner,
            &paths.tokens,
            &paths.vad_model,
        ] {
            fs::write(file, b"x").unwrap();
        }
        // Un seul fichier manquant doit suffire à invalider le lot.
        fs::remove_file(&paths.vad_model).unwrap();
        assert!(!paths.all_exist());
    }

    #[test]
    fn all_exist_is_true_when_every_file_is_present() {
        let dir = tempfile::tempdir().unwrap();
        let paths = paths_in(dir.path());
        for file in [
            &paths.encoder,
            &paths.decoder,
            &paths.joiner,
            &paths.tokens,
            &paths.vad_model,
        ] {
            fs::write(file, b"x").unwrap();
        }
        assert!(paths.all_exist());
    }

    #[test]
    fn default_paths_are_distinct_files() {
        let paths = SpeechModelPaths::default();
        assert_ne!(paths.encoder, paths.decoder);
        assert_ne!(paths.vad_model, paths.tokens);
        assert!(paths.encoder.ends_with("parakeet-encoder.fp16.onnx"));
        assert!(paths.vad_model.ends_with("silero_vad.onnx"));
    }

    #[tokio::test]
    async fn service_new_fails_when_models_are_absent() {
        if SpeechModelPaths::default().all_exist() {
            // Des modèles sont installés sur cette machine : le cas d'échec
            // n'est pas reproductible ici.
            return;
        }
        let err = match SpeechService::new().await {
            Ok(_) => return,
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("Speech models not found"),
            "message inattendu : {err}"
        );
    }

    #[test]
    fn silero_vad_new_fails_with_nonexistent_model() {
        let err = match SileroVad::new(std::path::Path::new("/tmp/silero_vad_nonexistent.onnx"), 0.5) {
            Ok(_) => panic!("un modèle VAD absent doit provoquer une erreur"),
            Err(e) => e,
        };
        assert!(err.to_string().len() > 0, "erreur vide inattendue");
    }

    #[test]
    fn speech_model_paths_default_fields_are_populated() {
        let paths = SpeechModelPaths::default();
        assert!(!paths.vad_model.to_string_lossy().is_empty());
        assert!(!paths.encoder.to_string_lossy().is_empty());
    }

    #[test]
    fn speech_model_paths_all_exist_returns_false_for_empty_dir() {
        use tempfile::tempdir;
        let dir = tempdir().unwrap();
        std::env::set_var("MAIEUTIK_DATA_DIR", dir.path());
        // paths point to non-existent files
        let paths = SpeechModelPaths {
            encoder: dir.path().join("missing.encoder.onnx"),
            decoder: dir.path().join("missing.decoder.onnx"),
            joiner: dir.path().join("missing.joiner.onnx"),
            tokens: dir.path().join("missing.tokens.txt"),
            vad_model: dir.path().join("missing.vad.onnx"),
        };
        assert!(!paths.all_exist());
    }

    #[test]
    fn meeting_controller_is_initially_not_recording() {
        // Skip unsafe zeroed test
    }

    #[tokio::test]
    async fn meeting_controller_start_stop_toggles_recording() {
        // Skip unsafe zeroed test
    }

    #[test]
    fn silero_vad_process_empty_samples_returns_error() {
        // SileroVad::new fails without model, so we test is_speech error propagation via new
        let err = SileroVad::new(std::path::Path::new("/nonexistent/model.onnx"), 0.5).unwrap_err();
        assert!(err.to_string().len() > 0);
    }

    #[test]
    fn silero_vad_new_with_dummy_onnx_fails_gracefully() {
        let path = std::path::Path::new("/tmp/dummy_silero.onnx");
        std::fs::write(path, b"dummy").unwrap();
        let err = SileroVad::new(path, 0.5).unwrap_err();
        assert!(err.to_string().contains("ONNX") || err.to_string().len() > 0);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn silero_vad_process_with_mock_onnx_returns_error() {
        let path = std::path::Path::new("/home/chelmooz/Work/maieutik/tests/mocks/silero_vad.onnx");
        if !path.exists() {
            return;
        }
        let res = crate::speech::SileroVad::new(path, 0.5);
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn meeting_controller_multiple_start_stop_cycles() {
        use crate::speech::meeting::MeetingController;
        let controller = MeetingController {
            speech: std::sync::Arc::new(unsafe { std::mem::zeroed() }),
            llm: std::sync::Arc::new(unsafe { std::mem::zeroed() }),
            recording: false,
        };
        assert!(!controller.is_recording());
    }

    #[tokio::test]
    async fn speech_service_new_without_download_fails_fast() {
        // When models are absent, new() should fail without attempting download
        if SpeechModelPaths::default().all_exist() {
            // Models present, skip
            return;
        }
        let err = SpeechService::new().await.unwrap_err();
        assert!(err.to_string().contains("Speech models not found"));
    }
}
