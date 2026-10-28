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



/// A5.1 RED: `chat` command should return exit code 3 (not implemented)
#[test]
fn chat_command_returns_not_implemented() {
    let (_dir, _work, home, cfg, data, cache) = setup_test_env(TempDir::new().unwrap());
    
    let output = run_maieutik(
        &["chat", "hello"],
        Some(vec![
            ("HOME", home.to_str().unwrap()),
            ("XDG_CONFIG_HOME", cfg.to_str().unwrap()),
            ("XDG_DATA_HOME", data.to_str().unwrap()),
            ("XDG_CACHE_HOME", cache.to_str().unwrap()),
            ("MAIEUTIK_INSTANCE_NAME", "test-a5-chat"),
        ]),
    );
    
    assert_eq!(output.status.code(), Some(3), "chat should return exit code 3");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("chat") || stderr.contains("non implémenté"), "stderr should mention chat/not implemented: {}", stderr);
}

/// A5.2 RED: `model list` command should return exit code 3
#[test]
fn model_list_returns_not_implemented() {
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
    
    assert_eq!(output.status.code(), Some(3), "model list should return exit code 3");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("model") || stderr.contains("non implémenté"), "stderr should mention model/not implemented: {}", stderr);
}

/// A5.3 RED: `model download` command should return exit code 3
#[test]
fn model_download_returns_not_implemented() {
    let (_dir, _work, home, cfg, data, cache) = setup_test_env(TempDir::new().unwrap());
    
    let output = run_maieutik(
        &["model", "download", "test-model"],
        Some(vec![
            ("HOME", home.to_str().unwrap()),
            ("XDG_CONFIG_HOME", cfg.to_str().unwrap()),
            ("XDG_DATA_HOME", data.to_str().unwrap()),
            ("XDG_CACHE_HOME", cache.to_str().unwrap()),
            ("MAIEUTIK_INSTANCE_NAME", "test-a5-model-dl"),
        ]),
    );
    
    assert_eq!(output.status.code(), Some(3), "model download should return exit code 3");
}

/// A5.4 RED: `model remove` command should return exit code 3
#[test]
fn model_remove_returns_not_implemented() {
    let (_dir, _work, home, cfg, data, cache) = setup_test_env(TempDir::new().unwrap());
    
    let output = run_maieutik(
        &["model", "remove", "test-model"],
        Some(vec![
            ("HOME", home.to_str().unwrap()),
            ("XDG_CONFIG_HOME", cfg.to_str().unwrap()),
            ("XDG_DATA_HOME", data.to_str().unwrap()),
            ("XDG_CACHE_HOME", cache.to_str().unwrap()),
            ("MAIEUTIK_INSTANCE_NAME", "test-a5-model-rm"),
        ]),
    );
    
    assert_eq!(output.status.code(), Some(3), "model remove should return exit code 3");
}

/// A5.5 RED: `model status` command should return exit code 3
#[test]
fn model_status_returns_not_implemented() {
    let (_dir, _work, home, cfg, data, cache) = setup_test_env(TempDir::new().unwrap());
    
    let output = run_maieutik(
        &["model", "status"],
        Some(vec![
            ("HOME", home.to_str().unwrap()),
            ("XDG_CONFIG_HOME", cfg.to_str().unwrap()),
            ("XDG_DATA_HOME", data.to_str().unwrap()),
            ("XDG_CACHE_HOME", cache.to_str().unwrap()),
            ("MAIEUTIK_INSTANCE_NAME", "test-a5-model-status"),
        ]),
    );
    
    assert_eq!(output.status.code(), Some(3), "model status should return exit code 3");
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
        .env("MAIEUTIK_EMBEDDER_TOKENIZER", dir.path().join("absent.json"))
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
    assert!(output.success(), "MCP server should exit cleanly on stdin close, got: {:?}", output);
}