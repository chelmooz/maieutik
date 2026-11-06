//! HTTP Downloader with resume support and streaming SHA256 verification.
//!
//! Features:
//! - Resume partial downloads using HTTP Range headers
//! - Streaming SHA256 verification (memory efficient)
//! - Progress callback support
//! - Configurable timeout

use anyhow::{Context, Result};
use reqwest::Client;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::time::Duration;
use tokio::fs::{File, OpenOptions};
use tokio::io::AsyncWriteExt;
use tokio_stream::StreamExt;

use crate::model::ProgressCallback;

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
        s.client = Self::create_client_with_timeout(timeout);
        s
    }

    fn create_client_with_timeout(timeout: Duration) -> Client {
        Client::builder()
            .user_agent(format!("maieutik/{}", env!("CARGO_PKG_VERSION")))
            .timeout(timeout)
            .build()
            .expect("Failed to create reqwest client")
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
}

impl Default for Downloader {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use tempfile::TempDir;
    use wiremock::matchers::{header, method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    #[tokio::test]
    async fn test_downloader_basic_download() {
        let server = MockServer::start().await;
        let test_data = b"Hello, world! This is test data for download.";
        let expected_hash = format!("{:x}", Sha256::digest(test_data));

        Mock::given(method("GET"))
            .and(path("/test.bin"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(test_data.to_vec()))
            .mount(&server)
            .await;

        let tmp = TempDir::new().expect("test");
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
            .expect("test");

        assert_eq!(hash, expected_hash);
        let content = tokio::fs::read(&dest).await.expect("test");
        assert_eq!(content, test_data);
    }

    #[tokio::test]
    async fn test_downloader_resumes_partial_download() {
        let server = MockServer::start().await;
        let test_data = b"0123456789ABCDEF"; // 16 bytes
        let second_half = &test_data[8..];

        // Simulate existing partial file (8 bytes)
        let tmp = TempDir::new().expect("test");
        let dest = tmp.path().join("resume.bin");
        tokio::fs::write(&dest, &test_data[..8])
            .await
            .expect("test");

        // Second request returns remaining bytes (simulating resume)
        Mock::given(method("GET"))
            .and(path("/resume.bin"))
            .and(header("Range", "bytes=8-"))
            .respond_with(ResponseTemplate::new(206).set_body_bytes(second_half.to_vec()))
            .mount(&server)
            .await;

        // Resume download
        let downloader = Downloader::new();
        let _ = downloader
            .download(&format!("{}/resume.bin", server.uri()), &dest, None, None)
            .await
            .expect("test");

        // Verify complete file
        let content = tokio::fs::read(&dest).await.expect("test");
        assert_eq!(content, test_data);
    }

    #[tokio::test]
    async fn test_downloader_verifies_sha256() {
        let server = MockServer::start().await;
        let test_data = b"data for sha256 verification";
        let correct_hash = format!("{:x}", Sha256::digest(test_data));
        let wrong_hash = "a".repeat(64);

        Mock::given(method("GET"))
            .and(path("/verify.bin"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(test_data.to_vec()))
            .mount(&server)
            .await;

        let tmp = TempDir::new().expect("test");
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
            .expect("test");
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
        let test_data = vec![0u8; 10_000]; // 10 KB

        Mock::given(method("GET"))
            .and(path("/progress.bin"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(test_data.clone()))
            .mount(&server)
            .await;

        let tmp = TempDir::new().expect("test");
        let dest = tmp.path().join("progress.bin");

        let progress_called = Arc::new(AtomicBool::new(false));
        let progress_called_clone = progress_called.clone();

        let downloader = Downloader::new();
        downloader
            .download(
                &format!("{}/progress.bin", server.uri()),
                &dest,
                None,
                Some(Box::new(move |downloaded, total| {
                    if downloaded > 0 && total > 0 {
                        progress_called_clone.store(true, Ordering::Relaxed);
                    }
                })),
            )
            .await
            .expect("test");

        assert!(progress_called.load(Ordering::Relaxed));
    }

    #[tokio::test]
    async fn test_downloader_creates_parent_dirs() {
        let server = MockServer::start().await;
        let test_data = b"nested test data";

        Mock::given(method("GET"))
            .and(path("/nested.bin"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(test_data.to_vec()))
            .mount(&server)
            .await;

        let tmp = TempDir::new().expect("test");
        let dest = tmp.path().join("a").join("b").join("c").join("nested.bin");

        let downloader = Downloader::new();
        downloader
            .download(&format!("{}/nested.bin", server.uri()), &dest, None, None)
            .await
            .expect("test");

        assert!(dest.exists());
    }

    #[tokio::test]
    async fn test_downloader_timeout_configurable() {
        let downloader = Downloader::with_timeout(Duration::from_secs(1));
        assert_eq!(downloader.timeout, Duration::from_secs(1));
    }
}
