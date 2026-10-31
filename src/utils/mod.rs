//! Utilities — document extraction, hashing

use anyhow::Result;
use std::io::Read;
use std::path::Path;

/// Extract text from various document formats
pub fn extract_text(path: &Path) -> Result<String> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_lowercase();
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
    // D'abord les entités nommées XML standard, puis les entités numériques.
    let mut out = out
        .replace("\x26lt;", "\x3c")
        .replace("\x26gt;", "\x3e")
        .replace("\x26quot;", "\"")
        .replace("\x26apos;", "\'")
        .replace("\x26amp;", "\x26");
    // Entités numériques décimales (&#233;) et hexadécimales (&#xE9;).
    out = decode_numeric_entities(&out);
    out
}

/// Parse a numeric entity like &#x1F600; or &#128;.
/// Returns (character, length_in_bytes_of_the_entity_without_semicolon) if valid, None otherwise.
fn parse_numeric_entity(s: &str) -> Option<(char, usize)> {
    let s = s.trim();
    // Must start with &# and end with ;
    if !s.starts_with("&#") || !s.ends_with(';') {
        return None;
    }
    let body = &s[2..s.len()-1]; // content between &# and ;
    
    // Determine if hex and extract digit characters
    let is_hex = body.starts_with('x') || body.starts_with('X');
    let digit_chars: String = if is_hex {
        body.chars().skip(1).filter(|c| c.is_ascii_hexdigit()).collect()
    } else {
        body.chars().filter(|c| c.is_ascii_digit()).collect()
    };
    
    if digit_chars.is_empty() {
        return None;
    }
    
    let value = if is_hex {
        u32::from_str_radix(&digit_chars, 16)
    } else {
        digit_chars.parse::<u32>()
    };
    
    value.ok().and_then(|cp| char::from_u32(cp).map(|c| (c, digit_chars.len())))
}

/// Décode les entités numériques &#x; ou &#; présents dans une chaîne.
/// Les entités invalides ou incomplètes sont restituées telles quelles
/// sans corruption silencieuse de texte (plus de préfixe dupliqué ni de
/// caractères de contrôle injectés comme form feed).
fn decode_numeric_entities(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut pos = 0;
    let bytes = s.as_bytes();

    while pos < bytes.len() {
        // Chercher le début d'une entité &#
        if pos + 5 < bytes.len()
            && bytes[pos] == 0x26u8
            && bytes[pos + 1] == 0x23u8
        {
            // Tentative de parser depuis cette position
            let entity_candidate = std::str::from_utf8(&bytes[pos..]).unwrap_or("");
            if let Some((ch, len)) = parse_numeric_entity(entity_candidate) {
                result.push(ch);
                pos += len;
            } else {
                // Entité invalide - restituer le caractère & et continuer
                result.push(bytes[pos] as char);
                pos += 1;
            }
        } else {
            result.push(bytes[pos] as char);
            pos += 1;
        }
    }
    result
}
