//! Model Management — Gestion des modèles LLM
//!
//! Architecture TDD :
//! - Types domaine (`ModelMetadata`, `ModelBackend`)
//! - Trait `ModelProvider`
//! - Factory pour créer le provider

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Backend du modèle (format ONNX, GGUF, etc.)
#[derive(Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ModelBackend {
    Onnx,
    Gguf,
    Safetensors,
}

impl std::fmt::Display for ModelBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModelBackend::Onnx => write!(f, "onnx"),
            ModelBackend::Gguf => write!(f, "gguf"),
            ModelBackend::Safetensors => write!(f, "safetensors"),
        }
    }
}

/// Statut d'un modèle
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub enum ModelStatus {
    NotInstalled,
    Installed { version: String, backend: ModelBackend },
    Corrupted { reason: String },
}

/// Metadata d'un modèle
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, PartialEq, Eq)]
pub struct ModelMetadata {
    /// Nom du modèle (ex: "bge-m3", "qwen2.5-7b")
    pub name: String,
    /// Version (ex: "1.0.0", "2024-06")
    pub version: String,
    /// SHA256 du fichier (64 hex chars)
    pub sha256: String,
    /// Taille en bytes
    pub size_bytes: u64,
    /// URL de téléchargement
    pub source_url: String,
    /// Quand le modèle a été installé (ISO 8601)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub installed_at: Option<String>,
    /// Backend du modèle
    pub backend: ModelBackend,
    /// Chemin local (calculé)
    #[serde(skip)]
    pub local_path: Option<PathBuf>,
}

impl ModelMetadata {
    /// Valide les métadonnées
    pub fn validate(&self) -> Result<(), ModelValidationError> {
        if self.name.is_empty() {
            return Err(ModelValidationError::EmptyName);
        }
        if self.version.is_empty() {
            return Err(ModelValidationError::EmptyVersion);
        }
        if self.source_url.is_empty() {
            return Err(ModelValidationError::EmptySourceUrl);
        }
        if self.sha256.len() != 64 {
            return Err(ModelValidationError::InvalidSha256);
        }
        if !self
            .sha256
            .chars()
            .all(|c| c.is_ascii_hexdigit())
        {
            return Err(ModelValidationError::InvalidSha256Format);
        }
        Ok(())
    }

    /// Retourne le chemin local du modèle
    pub fn path(&self) -> Option<&PathBuf> {
        self.local_path.as_ref()
    }

    /// Construit le chemin local à partir de la racine des modèles
    pub fn with_path(mut self, models_dir: &PathBuf) -> Self {
        let name = self.name.clone();
        let backend_str = match self.backend {
            ModelBackend::Onnx => "onnx",
            ModelBackend::Gguf => "gguf",
            ModelBackend::Safetensors => "safetensors",
        };
        let path = models_dir.join(&name).join(format!("model.{}", backend_str));
        self.local_path = Some(path);
        self
    }
}

/// Erreurs de validation
#[derive(Debug, Clone, thiserror::Error)]
pub enum ModelValidationError {
    #[error("Model name cannot be empty")]
    EmptyName,
    #[error("Model version cannot be empty")]
    EmptyVersion,
    #[error("Source URL cannot be empty")]
    EmptySourceUrl,
    #[error("SHA256 must be 64 hex characters")]
    InvalidSha256,
    #[error("SHA256 contains invalid characters")]
    InvalidSha256Format,
}

/// Callback de progression
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

/// Port pour le provider de modèles
#[async_trait::async_trait]
pub trait ModelProvider: Send + Sync + 'static {
    /// Liste les modèles installés
    async fn list(&self) -> Result<Vec<ModelMetadata>, ModelError>;

    /// Télécharge un modèle
    async fn download(
        &self,
        name: &str,
        version: Option<&str>,
        progress: Option<ProgressCallback>,
    ) -> Result<ModelMetadata, ModelError>;

    /// Installe un modèle depuis un fichier local
    async fn install(
        &self,
        metadata: ModelMetadata,
        source_path: &PathBuf,
        force: bool,
    ) -> Result<(), ModelError>;

    /// Supprime un modèle
    async fn remove(&self, name: &str) -> Result<(), ModelError>;

    /// Retourne le statut d'un modèle
    async fn status(&self, name: &str) -> Result<ModelStatus, ModelError>;
}

/// Erreurs du provider
#[derive(Debug, Clone, thiserror::Error)]
pub enum ModelError {
    #[error("Model '{0}' not found")]
    NotFound(String),

    #[error("Model '{0}' already installed")]
    AlreadyInstalled(String),

    #[error("Download failed: {0}")]
    DownloadFailed(String),

    #[error("Installation failed: {0}")]
    InstallationFailed(String),

    #[error("Verification failed: {0}")]
    VerificationFailed(String),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Validation error: {0}")]
    ValidationError(#[from] ModelValidationError),

    #[error("Registry error: {0}")]
    RegistryError(String),

    #[error("Internal error: {0}")]
    InternalError(String),
}

pub use registry::LocalRegistry;

mod registry;
pub mod cli;

pub use cli::{run_model_cmd, ModelCmd};

/// ModelManager - implémentation par défaut de ModelProvider
pub struct ModelManager {
    registry: LocalRegistry,
}

impl ModelManager {
    /// Crée un nouveau ModelManager avec le répertoire de données spécifié
    pub fn new(data_dir: &std::path::PathBuf) -> anyhow::Result<Self> {
        Ok(Self {
            registry: LocalRegistry::new(data_dir)?,
        })
    }
}

#[async_trait::async_trait]
impl ModelProvider for ModelManager {
    async fn list(&self) -> Result<Vec<ModelMetadata>, ModelError> {
        self.registry
            .list()
            .map_err(|e| ModelError::InternalError(e.to_string()))
    }

    async fn download(
        &self,
        name: &str,
        _version: Option<&str>,
        _progress: Option<ProgressCallback>,
    ) -> Result<ModelMetadata, ModelError> {
        Err(ModelError::DownloadFailed(format!(
            "Download not yet implemented for '{}'",
            name
        )))
    }

    async fn install(
        &self,
        _metadata: ModelMetadata,
        _source_path: &std::path::PathBuf,
        _force: bool,
    ) -> Result<(), ModelError> {
        Err(ModelError::InstallationFailed(
            "Install not yet implemented (stub)".to_string(),
        ))
    }

    async fn remove(&self, name: &str) -> Result<(), ModelError> {
        self.registry
            .remove(name)
            .map_err(|e| ModelError::InternalError(e.to_string()))
    }

    async fn status(&self, name: &str) -> Result<ModelStatus, ModelError> {
        self.registry
            .status(name)
            .map_err(|e| ModelError::InternalError(e.to_string()))
    }
}

mod tests {
    use super::*;

    #[test]
    fn test_model_metadata_serialization() {
        let metadata = ModelMetadata {
            name: "bge-m3".to_string(),
            version: "1.0.0".to_string(),
            sha256: "a".repeat(64),
            size_bytes: 1024,
            source_url: "https://huggingface.co/BAAI/bge-m3".to_string(),
            installed_at: None,
            backend: ModelBackend::Onnx,
            local_path: None,
        };

        let json = serde_json::to_string(&metadata).expect("serialize");
        let deserialized: ModelMetadata =
            serde_json::from_str(&json).expect("deserialize");

        assert_eq!(metadata.name, deserialized.name);
        assert_eq!(metadata.version, deserialized.version);
        assert_eq!(metadata.backend, deserialized.backend);
    }

    #[test]
    fn test_model_metadata_validation_rejects_empty_name() {
        let metadata = ModelMetadata {
            name: "".to_string(),
            version: "1.0.0".to_string(),
            sha256: "a".repeat(64),
            size_bytes: 1024,
            source_url: "https://example.com".to_string(),
            installed_at: None,
            backend: ModelBackend::Onnx,
            local_path: None,
        };

        assert!(matches!(
            metadata.validate(),
            Err(ModelValidationError::EmptyName)
        ));
    }

    #[test]
    fn test_model_metadata_validation_rejects_invalid_sha256() {
        let metadata = ModelMetadata {
            name: "test".to_string(),
            version: "1.0.0".to_string(),
            sha256: "not_valid_sha256".to_string(),
            size_bytes: 1024,
            source_url: "https://example.com".to_string(),
            installed_at: None,
            backend: ModelBackend::Onnx,
            local_path: None,
        };

        assert!(matches!(
            metadata.validate(),
            Err(ModelValidationError::InvalidSha256)
        ));
    }
}