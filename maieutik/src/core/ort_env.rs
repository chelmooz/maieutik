//! Shared ONNX Runtime environment.
//!
//! ONNX Runtime allows exactly one environment per process. Calling
//! `ort::init()` / `ort::init_from()` for every model fails the second time
//! with "only one environment is allowed per process" — which bit us in
//! practice with `Embedder` + `SileroVad` in the same process. Every model in
//! this crate builds its session from the environment returned here; no other
//! module may call `ort::init*` directly.

use anyhow::Result;
use ort::environment::Environment;
use std::path::Path;
use std::sync::OnceLock;

static ORT_ENVIRONMENT: OnceLock<Result<Environment, String>> = OnceLock::new();

/// Build (once) and return the process-wide ONNX Runtime environment.
///
/// With `ort`'s `load-dynamic`, the library path comes from `ORT_DYLIB_PATH`
/// (see `config::store::ort_dylib_path`); without it, the system loader is used.
pub fn ort_environment() -> Result<&'static Environment> {
    let env = ORT_ENVIRONMENT.get_or_init(|| {
        let builder = match crate::config::store::ort_dylib_path() {
            Some(path) => ort::init_from(path)
                .map_err(|e| format!("loading ONNX Runtime from ORT_DYLIB_PATH: {e}"))?,
            None => ort::init(),
        };
        builder.build().map_err(|e| e.to_string())
    });
    env.as_ref().map_err(|e| anyhow::anyhow!("{e}"))
}

/// Build a session for `model_path` with the crate's standard settings
/// (optimization level 3, configurable intra-op threads) from the shared
/// environment.
pub fn build_session(
    model_path: &Path,
    intra_threads: usize,
) -> anyhow::Result<ort::session::Session> {
    use ort::session::builder::GraphOptimizationLevel;

    let env = ort_environment()?;
    let session = ort::session::builder::SessionBuilder::new(env)
        .map_err(|e| anyhow::anyhow!("session builder: {e}"))?
        .with_optimization_level(GraphOptimizationLevel::Level3)
        .map_err(|e| anyhow::anyhow!("optimization level: {e}"))?
        .with_intra_threads(intra_threads)
        .map_err(|e| anyhow::anyhow!("intra threads: {e}"))?
        .commit_from_file(model_path)
        .map_err(|e| anyhow::anyhow!("load model {}: {e}", model_path.display()))?;
    Ok(session)
}
