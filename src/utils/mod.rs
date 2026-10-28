//! Utilities — document extraction, hashing

use anyhow::Result;
use std::io::Read;
use std::path::Path;

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
    // Absence de document.xml = échec d'extraction (jamais un texte vide).
    let mut doc = zip
        .by_name("word/document.xml")
        .map_err(|e| anyhow::anyhow!("docx sans word/document.xml: {e}"))?;
    let mut content = String::new();
    doc.read_to_string(&mut content)?;
    Ok(strip_xml_tags(&content))
}

fn extract_pdf(path: &Path) -> Result<String> {
    // Use pdf-extract
    pdf_extract::extract_text(path).map_err(Into::into)
}

fn strip_xml_tags(xml: &str) -> String {
    let mut out = String::new();
    let mut tag = String::new();
    let mut in_tag = false;
    for ch in xml.chars() {
        match ch {
            '\x3c' => {
                in_tag = true;
                tag.clear();
            }
            '\x3e' if in_tag => {
                in_tag = false;
                // Fin de paragraphe Word : saut de ligne.
                if tag == "/w:p" {
                    out.push(char::from(10u8));
                }
            }
            _ if in_tag => tag.push(ch),
            _ => out.push(ch),
        }
    }
    // Entités : amp en dernier, pour ne pas décoder deux fois.
    out.replace("\x26lt;", "\x3c")
        .replace("\x26gt;", "\x3e")
        .replace("\x26quot;", "\"")
        .replace("\x26apos;", "'")
        .replace("\x26amp;", "\x26")
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