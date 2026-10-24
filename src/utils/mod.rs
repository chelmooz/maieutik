//! Utilities — document extraction, hashing, process sandboxing

use anyhow::Result;
use std::io::Read;
use std::path::Path;
use tracing::{debug, warn};

/// Extract text from various document formats
pub fn extract_text(path: &Path) -> Result<String> {
    let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("").to_lowercase();
    match ext.as_str() {
        "txt" | "md" | "markdown" => extract_plain(path),
        "docx" => extract_docx(path),
        "pdf" => extract_pdf(path),
        _ => extract_plain(path),
    }
}

fn extract_plain(path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(path)?;
    Ok(content)
}

fn extract_docx(path: &Path) -> Result<String> {
    let file = std::fs::File::open(path)?;
    let mut zip = zip::ZipArchive::new(file)?;
    let mut text = String::new();

    // Read document.xml
    if let Ok(mut doc) = zip.by_name("word/document.xml") {
        let mut content = String::new();
        doc.read_to_string(&mut content)?;
        // Simple XML text extraction (remove tags)
        text = strip_xml_tags(&content);
    }
    Ok(text)
}

fn extract_pdf(path: &Path) -> Result<String> {
    // Use pdf-extract
    pdf_extract::extract_text(path).map_err(Into::into)
}

fn strip_xml_tags(xml: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in xml.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    // Decode common entities
    out.replace("<", "<")
        .replace(">", ">")
        .replace("&", "&")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
}

/// Compute blake3 hash of file
pub fn hash_file(path: &Path) -> Result<String> {
    let mut hasher = blake3::Hasher::new();
    let mut file = std::fs::File::open(path)?;
    std::io::copy(&mut file, &mut hasher)?;
    Ok(hasher.finalize().to_hex().to_string())
}

/// Compute blake3 hash of bytes
pub fn hash_bytes(data: &[u8]) -> String {
    blake3::hash(data).to_hex().to_string()
}

/// Process execution helper. NOT a security sandbox: no rlimits, no network
/// isolation, and only the direct child is killed on timeout.
pub mod sandbox {
    use anyhow::Result;
    use std::process::Stdio;
    use std::time::Duration;
    use tokio::process::Command;
    use tokio::io::{AsyncBufReadExt, BufReader};

    #[derive(Debug, Clone)]
    pub struct ExecResult {
        pub stdout: String,
        pub stderr: String,
        pub exit_code: Option<i32>,
        pub timed_out: bool,
    }

    pub async fn exec(
        cmd: &str,
        args: &[&str],
        input: Option<&str>,
        timeout: Duration,
        work_dir: Option<&std::path::Path>,
    ) -> Result<ExecResult> {
        let mut command = Command::new(cmd);
        command.args(args)
            .stdin(if input.is_some() { Stdio::piped() } else { Stdio::null() })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        if let Some(dir) = work_dir {
            command.current_dir(dir);
        }

        let mut child = command.spawn()?;

        if let Some(stdin_text) = input {
            if let Some(mut stdin) = child.stdin.take() {
                use tokio::io::AsyncWriteExt;
                stdin.write_all(stdin_text.as_bytes()).await?;
                stdin.shutdown().await?;
            }
        }

        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(result) => result?,
            Err(_) => {
                // `wait_with_output` took ownership of `child`; dropping the
                // future drops the child, and `kill_on_drop(true)` terminates it.
                return Ok(ExecResult {
                    stdout: String::new(),
                    stderr: format!("process timed out after {timeout:?}"),
                    exit_code: None,
                    timed_out: true,
                });
            }
        };

        Ok(ExecResult {
            stdout: String::from_utf8_lossy(&output.stdout).to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).to_string(),
            exit_code: output.status.code(),
            timed_out: false,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_bytes() {
        // BLAKE3 digest of the ASCII string "hello".
        assert_eq!(
            hash_bytes(b"hello"),
            "ea8f163db38682925e4491c5e58d4bb3506ef8c14eb78a86e908c5624a67200f"
        );
    }
}