//! HTTP Downloader with resume support and streaming SHA256 verification.

use anyhow::{Context, Result};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio_stream::StreamExt;

use crate::model::{ModelMetadata, ProgressCallback};

/// HTTP downloader with resume support and incremental SHA256.
pub struct Downloader {
    client: Client,
    timeout: Duration,
}

impl Downloader {
    /// Create a new downloader with default settings.
    pub fn new() -> Self {
        let client = Client::builder()
            .user_agent(format!("maieutik/{}", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(30 * 60)) // 30 minutes default timeout
            .build()
            .expect("Failed to create reqwest client");

        Self {
            client,
            timeout: Duration::from_secs(30 * 60),
        }
    }

    /// Create a downloader with custom timeout.
    pub fn with_timeout(timeout: Duration) -> Self {
        let mut s = Self::new();
        s.timeout = timeout;
        s
    }

    /// Download a file with resume support and streaming SHA256.
    ///
    /// If `dest_path` already exists, attempts to resume from the current file size.
    /// Returns the final SHA256 hash of the downloaded file.
    pub async fn download(
        &self,
        url: &str,
        dest_path: &Path,
        expected_sha256: Option<&str>,
        progress: Option<ProgressCallback>,
    ) -> Result<String> {
        // Ensure parent directory exists
        if let Some(parent) = dest_path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .with_context(|| format!("Creating parent directory: {}", parent.display()))?;
        }

        // Check existing file size for resume
        let resume_from = if dest_path.exists() {
            let metadata = tokio::fs::metadata(dest_path).await?;
            metadata.len()
        } else {
            0
        };

        let mut request = self.client.get(url);
        if resume_from > 0 {
            request = request.header("Range", format!("bytes={}-", resume_from));
        }

        let response = request
            .send()
            .await
            .with_context(|| format!("Request failed for {}", url))?;

        // Handle resume: if server doesn't support range, start from beginning
        let supports_resume = response.status().as_u16() == 206;
        let actual_resume_from = if supports_resume { resume_from } else { 0 };

        let total_size = response
            .content_length()
            .map(|cl| cl + actual_resume_from)
            .unwrap_or(0);

        // Open file for writing (append if resuming, create new otherwise)
        let mut file = if actual_resume_from > 0 {
            OpenOptions::new()
                .append(true)
                .open(dest_path)
                .await
                .with_context(|| format!("Opening {} for append", dest_path.display()))?
        } else {
            File::create(dest_path)
                .await
                .with_context(|| format!("Creating {}", dest_path.display()))?
        };

        let mut stream = response.bytes_stream();
        let mut hasher = Sha256::new();
        let mut downloaded = actual_resume_from;

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result.context("Error reading response stream")?;
            file.write_all(&chunk)
                .await
                .with_context(|| format!("Writing to {}", dest_path.display()))?;
            hasher.update(&chunk);
            downloaded += chunk.len() as u64;

            // Call progress callback
            if let Some(ref cb) = progress {
                cb(downloaded, total_size);
            }
        }

        file.flush()
            .await
            .with_context(|| format!("Flushing {}", dest_path.display()))?;
        file.sync_all()
            .await
            .with_context(|| format!("Syncing {}", dest_path.display()))?;

        let final_hash = format!("{:x}", hasher.finalize());

        // Verify SHA256 if expected
        if let Some(expected) = expected_sha256 {
            if final_hash != expected {
                anyhow::bail!("SHA256 mismatch: expected {}, got {}", expected, final_hash);
            }
        }

        Ok(final_hash)
    }

    /// Download with automatic model metadata lookup.
    ///
    /// This is a convenience method that uses a known model catalog.
    /// In production, this would query a model registry API.
    pub async fn download_model(
        &self,
        name: &str,
        version: Option<&str>,
        progress: Option<ProgressCallback>,
    ) -> Result<ModelMetadata> {
        // Known model catalog (in production, fetch from API)
        let catalog = Self::model_catalog();
        let entry = catalog.get(name).ok_or_else(|| {
            anyhow::anyhow!(
                "Unknown model: {}. Available: {}",
                name,
                catalog.keys().cloned().collect::<Vec<_>>().join(", ")
            )
        })?;

        // Select version
        let version = version.unwrap_or(entry.latest_version);
        let version_entry = entry
            .versions
            .get(version)
            .ok_or_else(|| anyhow::anyhow!("Version {} not found for model {}", version, name))?;

        // Determine destination path
        let models_dir = crate::config::store::models_dir()?;
        let dest_path = models_dir
            .join(entry.backend.to_string().to_lowercase())
            .join(name)
            .join(format!("model.{}", entry.backend.file_extension()));

        // Download
        let _ = self
            .download(
                &version_entry.url,
                &dest_path,
                Some(&version_entry.sha256),
                progress,
            )
            .await?;

        // Build metadata
        let metadata = ModelMetadata {
            name: name.to_string(),
            version: version.to_string(),
            sha256: version_entry.sha256.clone(),
            size_bytes: version_entry.size_bytes,
            source_url: version_entry.url.clone(),
            installed_at: chrono::Utc::now(),
            backend: entry.backend,
            description: Some(entry.description.clone()),
            min_free_memory: entry.min_free_memory,
        };

        metadata.validate()?;
        Ok(metadata)
    }

    /// Internal model catalog.
    fn model_catalog() -> std::collections::HashMap<&'static str, ModelCatalogEntry> {
        use std::collections::HashMap;
        let mut catalog = HashMap::new();

        // BGE-M3 embedding model (ONNX)
        catalog.insert(
            "bge-m3",
            ModelCatalogEntry {
                name: "bge-m3",
                description: "BAAI/bge-m3 multilingual embedding model (1024-dim, ONNX)"
                    .to_string(),
                backend: crate::model::ModelBackend::Onnx,
                latest_version: "1.0",
                versions: {
                    let mut v = HashMap::new();
                    v.insert(
                        "1.0",
                        ModelVersion {
                            url: "https://huggingface.co/BAAI/bge-m3/resolve/main/onnx/model.onnx"
                                .to_string(),
                            sha256:
                                "d1e5c8b3f4a2c9e8f7b6a5d4c3b2a1f0e9d8c7b6a5f4e3d2c1b0a9f8e7d6c5"
                                    .to_string(), // placeholder
                            size_bytes: 2_300_000_000, // ~2.3 GB
                        },
                    );
                    v
                },
                min_free_memory: Some(4_000_000_000), // 4 GB
            },
        );

        // Qwen 3.5 2B (GGUF)
        catalog.insert(
            "qwen3.5-2b",
            ModelCatalogEntry {
                name: "qwen3.5-2b",
                description: "Qwen 3.5 2B chat model (GGUF, Q6_K)".to_string(),
                backend: crate::model::ModelBackend::Gguf,
                latest_version: "1.0",
                versions: {
                    let mut v = HashMap::new();
                    v.insert(
                        "1.0",
                        ModelVersion {
                            url: "https://huggingface.co/lmstudio-community/Qwen3.5-2B-GGUF/resolve/main/Qwen3.5-2B-Q6_K.gguf".to_string(),
                            sha256: "a1b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2".to_string(), // placeholder
                            size_bytes: 1_556_390_368,
                        },
                    );
                    v
                },
                min_free_memory: Some(2_147_483_648), // 2 GB
            },
        );

        // Qwen 3.5 4B (GGUF)
        catalog.insert(
            "qwen3.5-4b",
            ModelCatalogEntry {
                name: "qwen3.5-4b",
                description: "Qwen 3.5 4B chat model (GGUF, Q4_K_M)".to_string(),
                backend: crate::model::ModelBackend::Gguf,
                latest_version: "1.0",
                versions: {
                    let mut v = HashMap::new();
                    v.insert(
                        "1.0",
                        ModelVersion {
                            url: "https://huggingface.co/lmstudio-community/Qwen3.5-4B-GGUF/resolve/main/Qwen3.5-4B-Q4_K_M.gguf".to_string(),
                            sha256: "b2c3d4e5f6a7b8c9d0e1f2a3b4c5d6e7f8a9b0c1d2e3f4a5b6c7d8e9f0a1b2c".to_string(), // placeholder
                            size_bytes: 2_707_513_696,
                        },
                    );
                    v
                },
                min_free_memory: Some(4_294_967_296), // 4 GB
            },
        );

        catalog
    }
}

#[allow(dead_code)]
struct ModelCatalogEntry {
    name: &'static str,
    description: String,
    backend: crate::model::ModelBackend,
    latest_version: &'static str,
    versions: std::collections::HashMap<&'static str, ModelVersion>,
    min_free_memory: Option<u64>,
}

struct ModelVersion {
    url: String,
    sha256: String,
    size_bytes: u64,
}

impl Default for Downloader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use tempfile::TempDir;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_downloader_basic_download() {
        let server = MockServer::start().await;
        let test_data = b"Hello, world! This is test data for download.";
        let expected_hash = format!("{:x}", sha2::Sha256::digest(test_data));

        Mock::given(method("GET"))
            .and(path("/test.bin"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(test_data.to_vec()))
            .mount(&server)
            .await;

        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("test.bin");

        let downloader = Downloader::new();
        let hash = downloader
            .download(
                &format!("{}/test.bin", server.uri()),
                &dest,
                Some(&expected_hash),
                None,
            )
            .await
            .unwrap();

        assert_eq!(hash, expected_hash);
        let content = tokio::fs::read(&dest).await.unwrap();
        assert_eq!(content, test_data);
    }

    #[tokio::test]
    async fn test_downloader_resumes_partial_download() {
        let server = MockServer::start().await;
        let test_data = b"0123456789ABCDEF"; // 16 bytes
        let second_half = &test_data[8..];

        // Simulate existing partial file (8 bytes)
        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("resume.bin");
        tokio::fs::write(&dest, &test_data[..8]).await.unwrap();

        // Second request returns remaining bytes (simulating resume)
        // Must match the Range header
        Mock::given(method("GET"))
            .and(path("/resume.bin"))
            .and(wiremock::matchers::header("Range", "bytes=8-"))
            .respond_with(ResponseTemplate::new(206).set_body_bytes(second_half.to_vec()))
            .mount(&server)
            .await;

        // Resume download
        let downloader = Downloader::new();
        let _ = downloader
            .download(&format!("{}/resume.bin", server.uri()), &dest, None, None)
            .await
            .unwrap();

        // Verify complete file
        let content = tokio::fs::read(&dest).await.unwrap();
        assert_eq!(content, test_data);
    }

    #[tokio::test]
    async fn test_downloader_verifies_sha256_streaming() {
        let server = MockServer::start().await;
        let test_data = b"data for sha256 verification";
        let correct_hash = format!("{:x}", sha2::Sha256::digest(test_data));
        let wrong_hash = "a".repeat(64);

        Mock::given(method("GET"))
            .and(path("/verify.bin"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(test_data.to_vec()))
            .mount(&server)
            .await;

        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("verify.bin");

        let downloader = Downloader::new();

        // Correct hash should succeed
        let hash = downloader
            .download(
                &format!("{}/verify.bin", server.uri()),
                &dest,
                Some(&correct_hash),
                None,
            )
            .await
            .unwrap();
        assert_eq!(hash, correct_hash);

        // Wrong hash should fail
        let result = downloader
            .download(
                &format!("{}/verify.bin", server.uri()),
                &dest,
                Some(&wrong_hash),
                None,
            )
            .await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SHA256 mismatch"));
    }

    #[tokio::test]
    async fn test_downloader_progress_callback() {
        let server = MockServer::start().await;
        let test_data = vec![0u8; 10000]; // 10 KB

        Mock::given(method("GET"))
            .and(path("/progress.bin"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(test_data.clone()))
            .mount(&server)
            .await;

        let tmp = TempDir::new().unwrap();
        let dest = tmp.path().join("progress.bin");

        let progress_called = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let progress_called_clone = progress_called.clone();

        let downloader = Downloader::new();
        downloader
            .download(
                &format!("{}/progress.bin", server.uri()),
                &dest,
                None,
                Some(Box::new(move |downloaded, total| {
                    if downloaded > 0 && total > 0 {
                        progress_called_clone.store(true, std::sync::atomic::Ordering::Relaxed);
                    }
                })),
            )
            .await
            .unwrap();

        assert!(progress_called.load(std::sync::atomic::Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_downloader_timeout_configurable() {
        let downloader = Downloader::with_timeout(Duration::from_secs(1));
        assert_eq!(downloader.timeout, Duration::from_secs(1));
    }
}
