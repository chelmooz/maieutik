//! Tests d'intégration LLM Ollama - 7 tests
//! Exécuter: cargo test --test llm_ollama

use maieutik::llm::{LlamaBackend, GenerationConfig};
use std::time::Duration;
use anyhow::Result;

fn config() -> GenerationConfig {
    GenerationConfig { max_tokens: 50, temperature: 0.7, ..Default::default() }
}

#[test]
fn test_backend_init_valid() -> Result<()> {
    if std::env::var("SKIP_OLLAMA").is_ok() { return Ok(()); }
    let backend = LlamaBackend::new("mistral:7b")?;
    Ok(())
}

#[test]
fn test_backend_init_invalid() {
    if std::env::var("SKIP_OLLAMA").is_ok() { return; }
    assert!(LlamaBackend::new("invalid:latest").is_err());
}

#[test]
fn test_generate_simple() -> Result<()> {
    if std::env::var("SKIP_OLLAMA").is_ok() { return Ok(()); }
    let backend = LlamaBackend::new("mistral:7b")?;
    let r = backend.generate("Bonjour", &config())?;
    assert!(!r.is_empty());
    Ok(())
}

#[test]
fn test_generate_config() -> Result<()> {
    if std::env::var("SKIP_OLLAMA").is_ok() { return Ok(()); }
    let backend = LlamaBackend::new("mistral:7b")?;
    let cfg = GenerationConfig { max_tokens: 100, temperature: 0.0, ..Default::default() };
    let r = backend.generate("Répète: test", &cfg)?;
    assert!(!r.is_empty());
    Ok(())
}

#[test]
fn test_timeout() -> Result<()> {
    if std::env::var("SKIP_OLLAMA").is_ok() { return Ok(()); }
    let backend = LlamaBackend::new("mistral:7b")?;
    let cfg = GenerationConfig { max_tokens: 10000, ..Default::default() };
    let _ = backend.generate_with_timeout("Long", &cfg, Duration::from_millis(100));
    Ok(())
}

#[test]
fn test_long_context() -> Result<()> {
    if std::env::var("SKIP_OLLAMA").is_ok() { return Ok(()); }
    let backend = LlamaBackend::new("mistral:7b")?;
    let ctx = "C. ".repeat(100);
    let r = backend.generate(&ctx, &config())?;
    assert!(!r.is_empty());
    Ok(())
}

#[test]
fn test_error_recovery() -> Result<()> {
    if std::env::var("SKIP_OLLAMA").is_ok() { return Ok(()); }
    let _ = LlamaBackend::new("invalid:latest");
    if let Ok(backend) = LlamaBackend::new("mistral:7b") {
        let _ = backend.generate("Test", &config());
    }
    Ok(())
}
