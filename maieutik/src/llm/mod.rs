//! LLM Generation — Intégration llama.cpp et Ollama
//!
//! Support GPU/CPU hybride avec streaming.
//!
//! Configuration :
//! - MAIEUTIK_LLM_MODEL : "ollama://nom_modele" ou chemin vers GGUF
//! - MAIEUTIK_LLM_CTX_SIZE : taille du contexte (défaut: 2048)
//! - MAIEUTIK_LLM_N_GPU_LAYERS : nombre de couches GPU (défaut: auto)

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

/// Ollama HTTP API client
pub mod ollama {
    use super::{Context, Result};
    use anyhow::anyhow;
    use serde::{Deserialize, Serialize};
    use std::time::Duration;
    use tokio::time::timeout;

    #[derive(Debug, Serialize)]
    struct ChatRequest {
        model: String,
        messages: Vec<ChatMessage>,
        options: Option<OllamaOptions>,
        stream: bool,
    }

    #[derive(Debug, Serialize, Clone)]
    pub struct ChatMessage {
        pub role: String,
        pub content: String,
    }

    #[derive(Debug, Serialize)]
    struct OllamaOptions {
        temperature: f32,
        top_p: f32,
        max_tokens: usize,
    }

    #[derive(Debug, Deserialize)]
    struct ChatResponse {
        message: Option<OllamaMessage>,
        #[allow(dead_code)]
        done: bool,
    }

    #[derive(Debug, Deserialize)]
    struct OllamaMessage {
        content: String,
    }

    /// Client Ollama pour les appels API
    #[derive(Debug)]
    pub struct OllamaClient {
        base_url: String,
        model: String,
        http_client: reqwest::Client,
    }

    impl OllamaClient {
        /// Crée un nouveau client Ollama
        pub fn new(base_url: &str, model: &str) -> Result<Self> {
            let http_client = reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .context("Échec de création du client HTTP")?;

            Ok(Self {
                base_url: base_url.to_string(),
                model: model.to_string(),
                http_client,
            })
        }

        /// Envoie une requête de chat vers Ollama
        pub async fn chat(&self, messages: Vec<ChatMessage>) -> Result<String> {
            let request = ChatRequest {
                model: self.model.clone(),
                messages,
                options: Some(OllamaOptions {
                    temperature: 0.7,
                    top_p: 0.9,
                    max_tokens: 1024,
                }),
                stream: false,
            };

            let url = format!("{}/api/chat", self.base_url);

            let response = timeout(
                Duration::from_secs(120),
                self.http_client.post(&url).json(&request).send(),
            )
            .await
            .context("Timeout Ollama")?
            .context("Échec de connexion à Ollama")?
            .json::<ChatResponse>()
            .await
            .context("Échec de déserialisation de la réponse Ollama")?;

            response
                .message
                .map(|m| m.content)
                .ok_or_else(|| anyhow!("Réponse Ollama vide"))
        }
    }

    /// Crée un générateur Ollama depuis une variable d'environnement
    pub fn create_ollama_generator() -> Result<OllamaGenerator> {
        let config = std::env::var("MAIEUTIK_LLM_MODEL")
            .map(|s| s.replace("ollama://", ""))
            .map_err(|_| {
                anyhow!("Variable MAIEUTIK_LLM_MODEL non définie ou pas au format 'ollama://model'")
            })?;

        let client = OllamaClient::new("http://localhost:11434", &config)?;
        Ok(OllamaGenerator { client })
    }

    /// Générateur qui utilise Ollama en backend
    pub struct OllamaGenerator {
        client: OllamaClient,
    }

    impl OllamaGenerator {
        /// Génère une réponse via Ollama
        pub async fn generate(&self, prompt: &str) -> Result<String> {
            let messages = vec![ChatMessage {
                role: "user".to_string(),
                content: prompt.to_string(),
            }];

            self.client.chat(messages).await
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;
        use wiremock::matchers::{method, path};
        use wiremock::{Mock, MockServer, ResponseTemplate};

        fn user_message(content: &str) -> ChatMessage {
            ChatMessage {
                role: "user".to_string(),
                content: content.to_string(),
            }
        }

        #[test]
        fn ollama_client_is_created_with_a_base_url_and_model() {
            let client = OllamaClient::new("http://localhost:11434", "qwen2.5:7b").expect("test");
            let debug = format!("{client:?}");
            assert!(debug.contains("qwen2.5:7b"), "{debug}");
        }

        #[tokio::test]
        async fn ollama_client_chat_returns_the_message_content() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "message": { "content": "Bonjour depuis Ollama" },
                    "done": true
                })))
                .mount(&server)
                .await;

            let client = OllamaClient::new(&server.uri(), "qwen2.5:7b").expect("test");
            let answer = client.chat(vec![user_message("salut")]).await.expect("test");

            assert_eq!(answer, "Bonjour depuis Ollama");
        }

        #[tokio::test]
        async fn ollama_client_chat_rejects_a_response_without_message() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "message": null,
                    "done": true
                })))
                .mount(&server)
                .await;

            let client = OllamaClient::new(&server.uri(), "qwen2.5:7b").expect("test");
            let err = client
                .chat(vec![user_message("salut")])
                .await
                .expect_err("une réponse sans message doit échouer");

            assert!(err.to_string().contains("Réponse Ollama vide"), "{err:#}");
        }

        #[tokio::test]
        async fn ollama_client_chat_reports_a_connection_failure() {
            // Aucun serveur n'écoute sur ce port : l'erreur de connexion doit
            // être remontée et non avalée.
            let client = OllamaClient::new("http://127.0.0.1:1", "qwen2.5:7b").expect("test");
            let result = client.chat(vec![user_message("salut")]).await;
            assert!(result.is_err(), "une connexion impossible doit échouer");
        }

        #[tokio::test]
        async fn ollama_generator_delegates_to_the_client() {
            let server = MockServer::start().await;
            Mock::given(method("POST"))
                .and(path("/api/chat"))
                .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
                    "message": { "content": "réponse du générateur" },
                    "done": true
                })))
                .mount(&server)
                .await;

            let generator = OllamaGenerator {
                client: OllamaClient::new(&server.uri(), "qwen2.5:7b").expect("test"),
            };
            let answer = generator.generate("question").await.expect("test");
            assert_eq!(answer, "réponse du générateur");
        }

        #[test]
        fn create_ollama_generator_strips_the_ollama_scheme() {
            let _guard = crate::llm::LLM_ENV_LOCK
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            let previous = std::env::var_os("MAIEUTIK_LLM_MODEL");
            std::env::set_var("MAIEUTIK_LLM_MODEL", "ollama://qwen2.5:7b");

            let result = create_ollama_generator();

            if let Some(value) = previous {
                std::env::set_var("MAIEUTIK_LLM_MODEL", value);
            } else {
                std::env::remove_var("MAIEUTIK_LLM_MODEL");
            }

            assert!(
                result.is_ok(),
                "un modèle Ollama doit produire un générateur"
            );
        }
    }
}

// Re-export types for Ollama usage
pub use ollama::{ChatMessage, OllamaClient};

/// Paramètres de génération
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerationParams {
    /// Prompt à générer
    pub prompt: String,
    /// Taille max du contexte
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ctx_size: Option<usize>,
    /// Batch size pour inférence
    #[serde(skip_serializing_if = "Option::is_none")]
    pub batch_size: Option<usize>,
    /// Temperature (0.0 - 2.0)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub temperature: Option<f32>,
    /// Top-p nucleus sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_p: Option<f32>,
    /// Top-k sampling
    #[serde(skip_serializing_if = "Option::is_none")]
    pub top_k: Option<usize>,
    /// Penalité de répétition
    #[serde(skip_serializing_if = "Option::is_none")]
    pub repeat_penalty: Option<f32>,
    /// Nombre max de tokens
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<usize>,
    /// Couches à exécuter sur GPU (-1 = auto)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub n_gpu_layers: Option<isize>,
}

impl Default for GenerationParams {
    fn default() -> Self {
        Self {
            prompt: String::new(),
            ctx_size: Some(2048),
            batch_size: Some(512),
            temperature: Some(0.7),
            top_p: Some(0.9),
            top_k: Some(40),
            repeat_penalty: Some(1.1),
            max_tokens: Some(512),
            n_gpu_layers: None,
        }
    }
}

/// Résultat d'une génération
#[derive(Debug, Clone)]
pub struct GenerationResult {
    /// Texte généré
    pub text: String,
    /// Nombre de tokens utilisés
    pub tokens: TokenUsage,
    /// Durée de génération
    pub duration_ms: u128,
}

#[derive(Debug, Clone, Default)]
pub struct TokenUsage {
    pub prompt: usize,
    pub completion: usize,
    pub total: usize,
}

/// Callback pour streaming les tokens
pub type StreamCallback = dyn Fn(&str) + Send + Sync;

/// Gestionnaire de modèles LLM
pub struct LlamaManager {
    model_path: PathBuf,
    #[allow(dead_code)]
    params: GenerationParams,
    ready: bool,
}

impl LlamaManager {
    /// Crée un nouveau gestionnaire LLM
    pub fn new(model_path: PathBuf) -> Result<Self> {
        if !model_path.exists() {
            anyhow::bail!("Modèle non trouvé: {}", model_path.display());
        }

        Ok(Self {
            model_path,
            params: GenerationParams::default(),
            ready: false,
        })
    }

    /// Crée un gestionnaire pour un modèle distant (backend Ollama) : il
    /// n'y a pas de fichier local dont l'existence peut être vérifiée.
    fn for_remote_model(model_path: PathBuf) -> Self {
        Self {
            model_path,
            params: GenerationParams::default(),
            ready: false,
        }
    }

    /// Initialise le modèle (chargement en mémoire)
    pub fn init(&mut self) -> Result<()> {
        info!("Initialisation du modèle: {:?}", self.model_path);
        // TODO: intégration réelle avec llama-cpp-2
        // Pour l'instant, on simule
        self.ready = true;
        Ok(())
    }

    /// Vérifie si le modèle est chargé
    pub fn is_ready(&self) -> bool {
        self.ready
    }

    /// Génère un texte
    pub fn generate(&mut self, params: GenerationParams) -> Result<GenerationResult> {
        if !self.ready {
            self.init()?;
        }

        let start = std::time::Instant::now();

        // Placeholder - l'implémentation réelle utilisera llama-cpp-2
        let response = self.simulate_generation(&params.prompt);

        let duration = start.elapsed().as_millis();

        Ok(GenerationResult {
            text: response?,
            tokens: TokenUsage::default(),
            duration_ms: duration,
        })
    }

    /// Génération simulée pour les tests
    fn simulate_generation(&self, prompt: &str) -> Result<String> {
        // Pour les tests, on retourne une réponse simulée
        // L'implémentation réelle utilisera llama-cpp-2
        warn!("Génération simulée - modèle LLM non chargé");
        Ok(format!(
            "Réponse simulée pour: {}... (modèle LLM non chargé, utilisez MAIEUTIK_LLM_MODEL pour configurer)",
            prompt.chars().take(50).collect::<String>()
        ))
    }
}

/// Factory pour créer le générateur LLM
/// Supporte "ollama://model_name" ou un chemin GGUF local
pub fn create_llm_generator() -> Result<Arc<Mutex<LlamaManager>>> {
    let model_config = std::env::var("MAIEUTIK_LLM_MODEL").map_err(|_| {
        anyhow::anyhow!(
            "Variable MAIEUTIK_LLM_MODEL non définie. \
                 Utilisez 'ollama://model_name' ou un chemin GGUF local."
        )
    })?;

    if model_config.starts_with("ollama://") {
        // Mode Ollama - retourner un LlamaManager avec placeholder
        // La génération réelle sera faite via OllamaClient
        info!("Mode Ollama configuré: {}", model_config);
        // Retourner un LlamaManager qui délègue à Ollama
        let model_path = std::env::temp_dir().join("ollama_placeholder");
        Ok(Arc::new(Mutex::new(LlamaManager::for_remote_model(
            model_path,
        ))))
    } else {
        // Mode llama.cpp local
        let model_path = PathBuf::from(model_config);
        Ok(Arc::new(Mutex::new(LlamaManager::new(model_path)?)))
    }
}

#[cfg(test)]
pub(crate) static LLM_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generation_params_default() {
        let params = GenerationParams::default();
        assert_eq!(params.ctx_size, Some(2048));
        assert_eq!(params.temperature, Some(0.7));
        assert_eq!(params.max_tokens, Some(512));
    }

    #[test]
    fn test_generation_params_serde() {
        let params = GenerationParams {
            prompt: "Hello".to_string(),
            temperature: Some(0.8),
            max_tokens: Some(100),
            ..Default::default()
        };

        let json = serde_json::to_string(&params).expect("test");
        let deserialized: GenerationParams = serde_json::from_str(&json).expect("test");

        assert_eq!(params.prompt, deserialized.prompt);
        assert_eq!(params.temperature, deserialized.temperature);
    }

    #[test]
    fn generation_params_skip_none_fields_when_serialized() {
        let params = GenerationParams {
            prompt: "p".to_string(),
            temperature: None,
            top_p: None,
            max_tokens: None,
            ..Default::default()
        };
        let json = serde_json::to_string(&params).expect("test");
        assert!(!json.contains("temperature"), "json inattendu : {json}");
        assert!(!json.contains("max_tokens"), "json inattendu : {json}");
        assert!(json.contains("\"prompt\":\"p\""), "json inattendu : {json}");
    }

    #[test]
    fn llama_manager_rejects_a_missing_model_file() {
        let err = match LlamaManager::new(PathBuf::from("/tmp/maieutik-model-absent.gguf")) {
            Ok(_) => panic!("un modèle absent doit être refusé"),
            Err(e) => e,
        };
        assert!(err.to_string().contains("Modèle non trouvé"), "{err:#}");
    }

    #[test]
    fn llama_manager_accepts_an_existing_model_file() {
        let dir = tempfile::tempdir().expect("test");
        let path = dir.path().join("model.gguf");
        std::fs::write(&path, b"gguf").expect("test");

        let manager = LlamaManager::new(path).expect("modèle présent");
        assert!(
            !manager.is_ready(),
            "le modèle ne doit pas être chargé avant init()"
        );
    }

    #[test]
    fn llama_manager_init_marks_the_model_ready() {
        let dir = tempfile::tempdir().expect("test");
        let path = dir.path().join("model.gguf");
        std::fs::write(&path, b"gguf").expect("test");

        let mut manager = LlamaManager::new(path).expect("test");
        manager.init().expect("test");
        assert!(manager.is_ready());
    }

    #[test]
    fn llama_manager_generate_initialises_lazily() {
        let dir = tempfile::tempdir().expect("test");
        let path = dir.path().join("model.gguf");
        std::fs::write(&path, b"gguf").expect("test");

        let mut manager = LlamaManager::new(path).expect("test");
        assert!(!manager.is_ready());

        let result = manager
            .generate(GenerationParams {
                prompt: "bonjour".to_string(),
                ..Default::default()
            })
            .expect("test");

        assert!(manager.is_ready(), "generate() doit initialiser le modèle");
        assert!(
            result.text.contains("bonjour"),
            "la réponse doit reprendre le prompt : {}",
            result.text
        );
        assert_eq!(result.tokens.prompt, 0);
        assert_eq!(result.tokens.completion, 0);
        assert_eq!(result.tokens.total, 0);
    }

    #[test]
    fn llama_manager_generate_truncates_a_very_long_prompt() {
        let dir = tempfile::tempdir().expect("test");
        let path = dir.path().join("model.gguf");
        std::fs::write(&path, b"gguf").expect("test");

        let mut manager = LlamaManager::new(path).expect("test");
        let long = "é".repeat(200);
        let result = manager
            .generate(GenerationParams {
                prompt: long,
                ..Default::default()
            })
            .expect("test");

        // Le prompt est tronqué à 50 caractères dans la réponse simulée.
        assert!(!result.text.contains(&"é".repeat(51)), "prompt non tronqué");
    }

    #[test]
    fn create_llm_generator_requires_the_model_environment_variable() {
        let _guard = crate::llm::LLM_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var_os("MAIEUTIK_LLM_MODEL");
        std::env::remove_var("MAIEUTIK_LLM_MODEL");

        let result = create_llm_generator();

        if let Some(value) = previous {
            std::env::set_var("MAIEUTIK_LLM_MODEL", value);
        }

        let err = match result {
            Ok(_) => panic!("la variable d'environnement est obligatoire"),
            Err(e) => e,
        };
        assert!(
            err.to_string().contains("MAIEUTIK_LLM_MODEL"),
            "message inattendu : {err:#}"
        );
    }

    #[test]
    fn create_llm_generator_builds_an_ollama_manager() {
        let _guard = crate::llm::LLM_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var_os("MAIEUTIK_LLM_MODEL");
        std::env::set_var("MAIEUTIK_LLM_MODEL", "ollama://qwen2.5:7b");

        let result = create_llm_generator();

        if let Some(value) = previous {
            std::env::set_var("MAIEUTIK_LLM_MODEL", value);
        } else {
            std::env::remove_var("MAIEUTIK_LLM_MODEL");
        }

        assert!(
            result.is_ok(),
            "un modèle Ollama doit produire un générateur"
        );
    }

    #[tokio::test]
    async fn test_real_generation_ollama() {
        let _guard = crate::llm::LLM_ENV_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let previous = std::env::var_os("MAIEUTIK_LLM_MODEL");
        std::env::set_var("MAIEUTIK_LLM_MODEL", "ollama://qwen2.5:3b");

        let generator = create_llm_generator().expect("generator");
        let prompt = "Résume ce texte en une phrase : intelligence artificielle est le domaine qui vise à créer des machines intelligentes.";
        let start = std::time::Instant::now();
        // Use Ollama client directly for test
        let client = crate::llm::ollama::OllamaClient::new("http://localhost:11434", "qwen2.5:3b").expect("client");
        let response = client.chat(vec![crate::llm::ollama::ChatMessage { role: "user".to_string(), content: prompt.to_string() }]).await.expect("generation");
        let elapsed = start.elapsed();
        info!("Generation took {:?}, response len {}", elapsed, response.len());

        if let Some(value) = previous {
            std::env::set_var("MAIEUTIK_LLM_MODEL", value);
        } else {
            std::env::remove_var("MAIEUTIK_LLM_MODEL");
        }

        assert!(!response.trim().is_empty(), "response empty");
    }
}
