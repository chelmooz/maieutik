//! Tests for CLI stubs and unimplemented commands (AUD-05).
//!
//! Verifies that unimplemented commands return explicit error codes.

use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use tempfile::TempDir;

fn run_maieutik(args: &[&str], env_overrides: Option<Vec<(&str, &str)>>) -> std::process::Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_maieutik"));
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(envs) = env_overrides {
        for (k, v) in envs {
            cmd.env(k, v);
        }
    }

    cmd.output().expect("Failed to run maieutik")
}

fn setup_test_env(dir: TempDir) -> (TempDir, PathBuf, PathBuf, PathBuf, PathBuf, PathBuf) {
    let work = dir.path().join("work");
    let home = dir.path().join("home");
    let cfg = dir.path().join("cfg");
    let data = dir.path().join("data");
    let cache = dir.path().join("cache");
    for d in [&work, &home, &cfg, &data, &cache] {
        fs::create_dir_all(d).unwrap();
    }
    (dir, work, home, cfg, data, cache)
}

/// `chat` sans générateur configuré ne doit jamais faire passer un echo pour
/// une réponse générée : stdout vide, diagnostic explicite, code 3.
#[test]
fn chat_without_llm_backend_exits_three() {
    let (_dir, _work, home, cfg, data, cache) = setup_test_env(TempDir::new().unwrap());

    let output = run_maieutik(
        &["chat", "hello"],
        Some(vec![
            ("HOME", home.to_str().unwrap()),
            ("XDG_CONFIG_HOME", cfg.to_str().unwrap()),
            ("XDG_DATA_HOME", data.to_str().unwrap()),
            ("XDG_CACHE_HOME", cache.to_str().unwrap()),
            ("MAIEUTIK_INSTANCE_NAME", "test-a5-chat"),
            // Neutralise un éventuel générateur hérité de l'environnement.
            ("MAIEUTIK_LLM_MODEL", ""),
        ]),
    );

    assert_eq!(
        output.status.code(),
        Some(3),
        "chat sans générateur doit sortir en 3, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.trim().is_empty(),
        "stdout doit rester vide : {}",
        stdout
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("aucun générateur configuré"),
        "stderr doit expliquer l'absence de générateur : {}",
        stderr
    );
    assert!(
        !stderr.contains("Réponse: hello"),
        "le message ne doit pas être présenté comme une réponse : {}",
        stderr
    );
}

/// A5.2 GREEN: `model list` command should work and return exit code 0
#[test]
fn model_list_works() {
    let (_dir, _work, home, cfg, data, cache) = setup_test_env(TempDir::new().unwrap());

    let output = run_maieutik(
        &["model", "list"],
        Some(vec![
            ("HOME", home.to_str().unwrap()),
            ("XDG_CONFIG_HOME", cfg.to_str().unwrap()),
            ("XDG_DATA_HOME", data.to_str().unwrap()),
            ("XDG_CACHE_HOME", cache.to_str().unwrap()),
            ("MAIEUTIK_INSTANCE_NAME", "test-a5-model-list"),
        ]),
    );

    assert_eq!(
        output.status.code(),
        Some(0),
        "model list should return exit code 0"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Should output "Aucun modèle installé." or empty list
    assert!(
        stdout.contains("Aucun") || stdout.trim().is_empty(),
        "stdout should mention no models: {}",
        stdout
    );
}

/// A5.3 GREEN: `model download` command should fail gracefully for unknown model
#[test]
fn model_download_unknown_model_fails() {
    let (_dir, _work, home, cfg, data, cache) = setup_test_env(TempDir::new().unwrap());

    let output = run_maieutik(
        &["model", "download", "unknown-model"],
        Some(vec![
            ("HOME", home.to_str().unwrap()),
            ("XDG_CONFIG_HOME", cfg.to_str().unwrap()),
            ("XDG_DATA_HOME", data.to_str().unwrap()),
            ("XDG_CACHE_HOME", cache.to_str().unwrap()),
            ("MAIEUTIK_INSTANCE_NAME", "test-a5-model-dl"),
        ]),
    );

    // Should fail with non-zero exit code (model not found)
    assert!(
        output.status.code().unwrap_or(-1) != 0,
        "model download unknown should fail"
    );
}

/// A5.4 GREEN: `model remove` command should fail gracefully for unknown model
#[test]
fn model_remove_unknown_model_fails() {
    let (_dir, _work, home, cfg, data, cache) = setup_test_env(TempDir::new().unwrap());

    let output = run_maieutik(
        &["model", "remove", "unknown-model", "--force"],
        Some(vec![
            ("HOME", home.to_str().unwrap()),
            ("XDG_CONFIG_HOME", cfg.to_str().unwrap()),
            ("XDG_DATA_HOME", data.to_str().unwrap()),
            ("XDG_CACHE_HOME", cache.to_str().unwrap()),
            ("MAIEUTIK_INSTANCE_NAME", "test-a5-model-rm"),
        ]),
    );

    // Should fail with non-zero exit code (model not found)
    assert!(
        output.status.code().unwrap_or(-1) != 0,
        "model remove unknown should fail"
    );
}

/// `model status` reports a missing model on stderr and exits 1.
#[test]
fn model_status_not_installed() {
    let (_dir, _work, home, cfg, data, cache) = setup_test_env(TempDir::new().unwrap());

    let output = run_maieutik(
        &["model", "status", "unknown-model"],
        Some(vec![
            ("HOME", home.to_str().unwrap()),
            ("XDG_CONFIG_HOME", cfg.to_str().unwrap()),
            ("XDG_DATA_HOME", data.to_str().unwrap()),
            ("XDG_CACHE_HOME", cache.to_str().unwrap()),
            ("MAIEUTIK_INSTANCE_NAME", "test-a5-model-status"),
        ]),
    );

    assert_eq!(
        output.status.code(),
        Some(1),
        "model status should return exit code 1 for a missing model"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not installed"),
        "stderr should mention not installed: {}",
        stderr
    );
}

/// A5.6 GREEN: `--mcp-stdio` should start and exit cleanly on stdin close
#[test]
fn mcp_stdio_works() {
    let dir = TempDir::new().unwrap();
    let work = dir.path().join("work");
    let home = dir.path().join("home");
    let cfg = dir.path().join("cfg");
    let data = dir.path().join("data");
    let cache = dir.path().join("cache");
    for d in [&work, &home, &cfg, &data, &cache] {
        fs::create_dir_all(d).unwrap();
    }

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_maieutik"));
    cmd.arg("--mcp-stdio")
        .current_dir(&work)
        .env_clear()
        .env("HOME", &home)
        .env("XDG_CONFIG_HOME", &cfg)
        .env("XDG_DATA_HOME", &data)
        .env("XDG_CACHE_HOME", &cache)
        .env("MAIEUTIK_INSTANCE_NAME", "test-a5-mcp-stdio")
        .env("MAIEUTIK_EMBEDDER_MODEL", dir.path().join("absent.onnx"))
        .env(
            "MAIEUTIK_EMBEDDER_TOKENIZER",
            dir.path().join("absent.json"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let mut child = cmd.spawn().expect("Failed to start maieutik --mcp-stdio");

    // Give server time to start
    std::thread::sleep(std::time::Duration::from_millis(200));

    // Close stdin to trigger graceful shutdown (server should exit on EOF)
    drop(child.stdin.take());

    // Wait for process to exit
    let output = child.wait().expect("Failed to wait for process");

    // Should exit cleanly with code 0
    assert!(
        output.success(),
        "MCP server should exit cleanly on stdin close, got: {:?}",
        output
    );
}
