//! Model Installer — Atomic installation with SHA256 verification.

use anyhow::{Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tracing::{debug, info};

use crate::model::{LocalRegistry, ModelMetadata, ModelStatus};

/// Installer handles atomic model installation and verification.
pub struct Installer {
    registry: LocalRegistry,
    models_dir: PathBuf,
}

impl Installer {
    /// Create a new installer with the default registry and models directory.
    pub async fn new() -> Result<Self> {
        let registry = LocalRegistry::new().await?;
        let models_dir = crate::config::store::models_dir()?;
        Ok(Self {
            registry,
            models_dir,
        })
    }

    /// Create an installer with custom registry and models directory (for testing).
    pub fn with_registry(registry: LocalRegistry, models_dir: PathBuf) -> Self {
        Self {
            registry,
            models_dir,
        }
    }

    /// Install a model from a source file to the registry.
    ///
    /// Verifies SHA256, then atomically moves the file to the final location.
    /// Updates the registry on success.
    pub async fn install(&self, metadata: &ModelMetadata, source_path: &Path) -> Result<()> {
        metadata.validate().context("Invalid model metadata")?;

        // Verify source file exists
        if !source_path.exists() {
            anyhow::bail!("Source file not found: {}", source_path.display());
        }

        // Verify SHA256 of source file
        let actual_hash = Self::compute_sha256(source_path).await?;
        if actual_hash != metadata.sha256 {
            anyhow::bail!(
                "SHA256 verification failed: expected {}, got {}",
                metadata.sha256,
                actual_hash
            );
        }

        // Determine destination path
        let dest_path = metadata.local_path(&self.models_dir);

        // Ensure parent directory exists
        if let Some(parent) = dest_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("Creating destination directory: {}", parent.display()))?;
        }

        // Check if destination already exists
        if dest_path.exists() {
            // Verify existing file
            let existing_hash = Self::compute_sha256(&dest_path).await?;
            if existing_hash == metadata.sha256 {
                info!(
                    "Model {} already installed with correct hash",
                    metadata.name
                );
                // Still update registry in case it was missing
                self.registry.add(metadata.clone());
                self.registry.persist().await?;
                return Ok(());
            } else {
                anyhow::bail!("Destination exists but has different hash. Use force to overwrite.");
            }
        }

        // Atomic move: source -> destination
        // Use rename for atomicity (same filesystem) or copy+sync+remove for cross-fs
        Self::atomic_move(source_path, &dest_path).await?;

        // Verify installed file
        let installed_hash = Self::compute_sha256(&dest_path).await?;
        if installed_hash != metadata.sha256 {
            // Cleanup on verification failure
            let _ = fs::remove_file(&dest_path);
            anyhow::bail!("Post-install verification failed: hash mismatch");
        }

        // Register the model
        self.registry.add(metadata.clone());
        self.registry.persist().await?;

        info!(
            "Installed model: {} v{} at {}",
            metadata.name,
            metadata.version,
            dest_path.display()
        );
        Ok(())
    }

    /// Compute SHA256 of a file.
    async fn compute_sha256(path: &Path) -> Result<String> {
        let mut file = File::open(path).await?;
        let mut hasher = Sha256::new();
        let mut buffer = vec![0u8; 8192];

        loop {
            let n = file.read(&mut buffer).await?;
            if n == 0 {
                break;
            }
            hasher.update(&buffer[..n]);
        }

        Ok(format!("{:x}", hasher.finalize()))
    }

    /// Atomically move a file (rename on same fs, copy+sync+remove on cross-fs).
    async fn atomic_move(src: &Path, dst: &Path) -> Result<()> {
        // Try rename first (atomic on same filesystem)
        match fs::rename(src, dst) {
            Ok(()) => {
                debug!("Atomically moved {} to {}", src.display(), dst.display());
                return Ok(());
            }
            Err(e) if e.raw_os_error() == Some(libc::EXDEV) => {
                // Cross-device: copy + sync + remove
                debug!(
                    "Cross-device move, copying {} to {}",
                    src.display(),
                    dst.display()
                );
            }
            Err(e) => {
                return Err(e).with_context(|| {
                    format!("Failed to rename {} to {}", src.display(), dst.display())
                });
            }
        }

        // Copy file
        fs::copy(src, dst)
            .with_context(|| format!("Copying {} to {}", src.display(), dst.display()))?;

        // Sync destination
        let dst_file = File::open(dst).await?;
        dst_file.sync_all().await?;

        // Remove source
        fs::remove_file(src).with_context(|| format!("Removing source {}", src.display()))?;

        debug!("Copied and verified {} to {}", src.display(), dst.display());
        Ok(())
    }

    /// Remove a model from the registry and delete its file.
    pub async fn remove(&self, name: &str) -> Result<()> {
        let metadata = self
            .registry
            .get(name)
            .ok_or_else(|| anyhow::anyhow!("Model not found in registry: {}", name))?;

        // Delete the model file
        let file_path = metadata.local_path(&self.models_dir);
        if file_path.exists() {
            fs::remove_file(&file_path)
                .with_context(|| format!("Removing model file: {}", file_path.display()))?;
            info!("Removed model file: {}", file_path.display());
        }

        // Remove from registry
        self.registry.remove(name);
        self.registry.persist().await?;

        info!("Removed model: {}", name);
        Ok(())
    }

    /// Get the status of a model.
    pub async fn status(&self, name: &str) -> ModelStatus {
        self.registry.get_status(name, &self.models_dir).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ModelBackend;
    use tempfile::TempDir;

    #[tokio::test]
    async fn test_installer_atomic_move() {
        let tmp = TempDir::new().unwrap();
        let models_dir = tmp.path().join("models");
        let source = tmp.path().join("source.bin");
        let dest = models_dir.join("onnx").join("test").join("model.onnx");

        // Create source file
        tokio::fs::write(&source, b"test data").await.unwrap();

        let registry = LocalRegistry::with_path(tmp.path().join("registry.json"))
            .await
            .unwrap();
        let installer = Installer::with_registry(registry, models_dir);

        let metadata = ModelMetadata {
            name: "test".to_string(),
            version: "1.0".to_string(),
            sha256: format!("{:x}", sha2::Sha256::digest(b"test data")),
            size_bytes: 9,
            source_url: "http://example.com".to_string(),
            installed_at: chrono::Utc::now(),
            backend: ModelBackend::Onnx,
            description: None,
            min_free_memory: None,
        };

        installer.install(&metadata, &source).await.unwrap();

        // Verify destination exists and source is gone
        assert!(dest.exists());
        assert!(!source.exists());

        // Verify content
        let content = tokio::fs::read(&dest).await.unwrap();
        assert_eq!(content, b"test data");
    }

    #[tokio::test]
    async fn test_installer_verifies_sha256_before_commit() {
        let tmp = TempDir::new().unwrap();
        let models_dir = tmp.path().join("models");
        let source = tmp.path().join("source.bin");

        tokio::fs::write(&source, b"wrong data").await.unwrap();

        let registry = LocalRegistry::with_path(tmp.path().join("registry.json"))
            .await
            .unwrap();
        let installer = Installer::with_registry(registry, models_dir);

        let metadata = ModelMetadata {
            name: "test".to_string(),
            version: "1.0".to_string(),
            sha256: format!("{:x}", sha2::Sha256::digest(b"correct data")),
            size_bytes: 9,
            source_url: "http://example.com".to_string(),
            installed_at: chrono::Utc::now(),
            backend: ModelBackend::Onnx,
            description: None,
            min_free_memory: None,
        };

        let result = installer.install(&metadata, &source).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("SHA256 verification failed"));

        // Destination should not exist
        let dest = metadata.local_path(&tmp.path().join("models"));
        assert!(!dest.exists());
    }

    #[tokio::test]
    async fn test_installer_creates_parent_dirs() {
        let tmp = TempDir::new().unwrap();
        let models_dir = tmp.path().join("models");
        let source = tmp.path().join("source.bin");

        tokio::fs::write(&source, b"test").await.unwrap();

        let registry = LocalRegistry::with_path(tmp.path().join("registry.json"))
            .await
            .unwrap();
        let installer = Installer::with_registry(registry, models_dir.clone());

        let metadata = ModelMetadata {
            name: "deeply/nested/model".to_string(),
            version: "1.0".to_string(),
            sha256: format!("{:x}", sha2::Sha256::digest(b"test")),
            size_bytes: 4,
            source_url: "http://example.com".to_string(),
            installed_at: chrono::Utc::now(),
            backend: ModelBackend::Onnx,
            description: None,
            min_free_memory: None,
        };

        installer.install(&metadata, &source).await.unwrap();

        let dest = metadata.local_path(&models_dir);
        assert!(dest.exists());
        assert!(dest.parent().unwrap().exists());
    }

    #[tokio::test]
    async fn test_installer_refuses_overwrite_without_force() {
        let tmp = TempDir::new().unwrap();
        let models_dir = tmp.path().join("models");
        let source = tmp.path().join("source.bin");
        let dest = models_dir.join("onnx").join("test").join("model.onnx");

        // Create existing file with different content
        fs::create_dir_all(dest.parent().unwrap()).unwrap();
        tokio::fs::write(&dest, b"existing").await.unwrap();
        tokio::fs::write(&source, b"new content").await.unwrap();

        let registry = LocalRegistry::with_path(tmp.path().join("registry.json"))
            .await
            .unwrap();
        let installer = Installer::with_registry(registry, models_dir);

        let metadata = ModelMetadata {
            name: "test".to_string(),
            version: "1.0".to_string(),
            sha256: format!("{:x}", sha2::Sha256::digest(b"new content")),
            size_bytes: 10,
            source_url: "http://example.com".to_string(),
            installed_at: chrono::Utc::now(),
            backend: ModelBackend::Onnx,
            description: None,
            min_free_memory: None,
        };

        let result = installer.install(&metadata, &source).await;
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .to_string()
            .contains("Destination exists"));
    }

    #[tokio::test]
    async fn test_installer_registers_in_registry() {
        let tmp = TempDir::new().unwrap();
        let models_dir = tmp.path().join("models");
        let source = tmp.path().join("source.bin");

        tokio::fs::write(&source, b"test data").await.unwrap();

        let registry = LocalRegistry::with_path(tmp.path().join("registry.json"))
            .await
            .unwrap();
        let installer = Installer::with_registry(registry.clone(), models_dir);

        let metadata = ModelMetadata {
            name: "test".to_string(),
            version: "1.0".to_string(),
            sha256: format!("{:x}", sha2::Sha256::digest(b"test data")),
            size_bytes: 9,
            source_url: "http://example.com".to_string(),
            installed_at: chrono::Utc::now(),
            backend: ModelBackend::Onnx,
            description: None,
            min_free_memory: None,
        };

        installer.install(&metadata, &source).await.unwrap();

        // Verify registry has the model
        let registered = registry.get("test");
        assert!(registered.is_some());
        assert_eq!(registered.unwrap().name, "test");
    }
}
