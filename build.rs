//! Build script for Maieutik.
//!
//! Downloading models during a build makes the build non-reproducible and fails
//! offline, so it is opt-in. Set `MAIEUTIK_DOWNLOAD_MODELS=1` to fetch the small
//! models into `$OUT_DIR/models`. By default nothing is downloaded and nothing is
//! written outside `OUT_DIR`; the application downloads models at runtime into
//! the user data directory (`config::store::models_dir()`).

use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=MAIEUTIK_DOWNLOAD_MODELS");

    if env::var("MAIEUTIK_DOWNLOAD_MODELS").as_deref() != Ok("1") {
        println!(
            "cargo:warning=Skipping model download (set MAIEUTIK_DOWNLOAD_MODELS=1 to enable); \
             models are fetched at runtime instead"
        );
        return;
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR is always set by cargo"));
    let models_dir = out_dir.join("models");
    if let Err(e) = fs::create_dir_all(&models_dir) {
        println!("cargo:warning=Cannot create {}: {e}", models_dir.display());
        return;
    }

    // Small models only (VAD, vocabulary).
    let small_models = [
        (
            "silero_vad.onnx",
            "https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad.onnx",
        ),
        (
            "vocab.txt",
            "https://huggingface.co/ako101/parakeet-tdt-0.6b-v3-sherpa-onnx-fp16/resolve/main/tokens.txt",
        ),
    ];

    for (name, url) in small_models {
        let dest = models_dir.join(name);
        if dest.exists() {
            continue;
        }
        println!("cargo:warning=Downloading {name}...");
        match download_file_blocking(url, &dest) {
            Ok(()) => println!("cargo:warning=Downloaded {name}"),
            Err(e) => println!(
                "cargo:warning=Failed to download {name}: {e}; \
                 the application will download it at runtime"
            ),
        }
    }
}

fn download_file_blocking(url: &str, dest: &PathBuf) -> Result<(), Box<dyn std::error::Error>> {
    let resp = reqwest::blocking::get(url)?.error_for_status()?;
    let bytes = resp.bytes()?;
    fs::write(dest, &bytes)?;
    Ok(())
}
