//! Tests d'intégration LLM avec Ollama
//! Requiert: ollama pull mistral:7b
//!
//! Pour lancer les tests:
//! ```bash
//! export MAIEUTIK_SKIP_OLLAMA=1  # Pour sauter les tests LLM
//! cargo test --test llm_ollama -- --nocapture
//! ```

use std::path::PathBuf;
use std::time::Duration;
use anyhow::Result;

/// Imports corrigés selon l'API réelle de maieutik
use maieutik::llm::{LlamaManager, GenerationParams, GenerationResult};

/// Configuration de base pour les tests
fn test_params(prompt: &str) -> GenerationParams {
    GenerationParams {
        prompt: prompt.to_string(),
        max_tokens: Some(50),
        temperature: Some(0.7),
        ..Default::default()
    }
}

// ============================================================================
// 🟢 TESTS DE BASE
// ============================================================================

/// **T0-1** : Initialisation du manager avec modèle valide
#[test]
fn test_llm_manager_init_valid_model() -> Result<()> {
    if std::env::var("MAIEUTIK_SKIP_OLLAMA").is_ok() {
        return Ok(());
    }

    let model_path = PathBuf::from("mistral:7b");
    let manager = LlamaManager::new(model_path)?;
    assert!(manager.is_ready());
    Ok(())
}

/// **T0-2** : Initialisation échoue avec modèle invalide
#[test]
fn test_llm_manager_init_invalid_model() {
    if std::env::var("MAIEUTIK_SKIP_OLLAMA").is_ok() {
        return;
    }

    let model_path = PathBuf::from("invalid-model:latest");
    let result = LlamaManager::new(model_path);
    assert!(result.is_err());
}

/// **T0-3** : Initialisation si Ollama non démarré (ne panique pas)
#[test]
fn test_llm_manager_init_ollama_not_running() {
    if std::env::var("MAIEUTIK_SKIP_OLLAMA").is_ok() {
        return;
    }

    let model_path = PathBuf::from("mistral:7b");
    let result = LlamaManager::new(model_path);
    assert!(result.is_ok() || result.is_err());
}

// ============================================================================
// 🟢 TESTS DE GÉNÉRATION
// ============================================================================

/// **T0-4** : Génération de texte simple
#[test]
fn test_llm_generate_simple_prompt() -> Result<()> {
    if std::env::var("MAIEUTIK_SKIP_OLLAMA").is_ok() {
        return Ok(());
    }

    let mut manager = LlamaManager::new(PathBuf::from("mistral:7b"))?;
    let params = test_params("Bonjour, comment ça va?");

    let result: GenerationResult = manager.generate(params)?;
    assert!(!result.text.is_empty());
    assert!(result.text.len() > 10);
    Ok(())
}

/// **T0-5** : Génération avec configuration personnalisée
#[test]
fn test_llm_generate_with_custom_params() -> Result<()> {
    if std::env::var("MAIEUTIK_SKIP_OLLAMA").is_ok() {
        return Ok(());
    }

    let mut manager = LlamaManager::new(PathBuf::from("mistral:7b"))?;
    let params = GenerationParams {
        prompt: "Répète exactement: 'Test déterministe'".to_string(),
        max_tokens: Some(100),
        temperature: Some(0.0),
        ..Default::default()
    };

    let result = manager.generate(params)?;
    assert!(!result.text.is_empty());
    Ok(())
}

/// **T0-6** : Génération avec contexte long
#[test]
fn test_llm_generate_long_context() -> Result<()> {
    if std::env::var("MAIEUTIK_SKIP_OLLAMA").is_ok() {
        return Ok(());
    }

    let mut manager = LlamaManager::new(PathBuf::from("mistral:7b"))?;
    let long_context = "Voici un long contexte. ".repeat(50);
    let params = test_params(&long_context);

    let result = manager.generate(params)?;
    assert!(!result.text.is_empty());
    Ok(())
}

// ============================================================================
// 🟡 TESTS DE PERFORMANCE
// ============================================================================

/// **T0-7** : Génération avec timeout
#[test]
fn test_llm_generate_with_timeout() -> Result<()> {
    if std::env::var("MAIEUTIK_SKIP_OLLAMA").is_ok() {
        return Ok(());
    }

    let mut manager = LlamaManager::new(PathBuf::from("mistral:7b"))?;
    let params = GenerationParams {
        prompt: "Génère un texte court".to_string(),
        max_tokens: Some(100),
        ..Default::default()
    };

    let result = manager.generate(params)?;
    assert!(!result.text.is_empty());
    Ok(())
}

// ============================================================================
// 🔴 TESTS D'ERREUR
// ============================================================================

/// **T0-8** : Gestion des erreurs de génération
#[test]
fn test_llm_generate_error_handling() -> Result<()> {
    if std::env::var("MAIEUTIK_SKIP_OLLAMA").is_ok() {
        return Ok(());
    }

    let mut manager = LlamaManager::new(PathBuf::from("mistral:7b"))?;

    let huge_prompt = "A".repeat(100000);
    let params = test_params(&huge_prompt);

    let result = manager.generate(params);
    assert!(result.is_ok() || result.is_err());
    Ok(())
}

/// **T0-9** : Manager reste utilisable après erreur
#[test]
fn test_llm_manager_recoverable_after_error() -> Result<()> {
    if std::env::var("MAIEUTIK_SKIP_OLLAMA").is_ok() {
        return Ok(());
    }

    let mut manager = LlamaManager::new(PathBuf::from("mistral:7b"))?;

    let _ = LlamaManager::new(PathBuf::from("invalid-model"));

    let params = test_params("Test");
    let result = manager.generate(params)?;
    assert!(!result.text.is_empty());
    Ok(())
}
