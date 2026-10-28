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

/// Décode les entités XML numériques : &#123; et &#x1F;.
/// Ne panique pas sur des valeurs invalides ; laisse l'entité telle quelle.
fn decode_numeric_entities(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\x26' && chars.peek() == Some(&'\x23') {
            // Possible numeric entity: &#
            chars.next(); // consume '#'
            let mut hex = false;
            if chars.peek() == Some(&'x') || chars.peek() == Some(&'X') {
                hex = true;
                chars.next(); // consume 'x' or 'X'
            }
            // Collect digits
            let mut digits = String::new();
            let mut entity_end = false;
            for d in chars.by_ref() {
                if d == ';' {
                    entity_end = true;
                    break;
                }
                if d.is_ascii_digit() || (hex && d.is_ascii_hexdigit()) {
                    digits.push(d);
                } else {
                    // Invalid character in entity - restore and continue
                    result.push('\x26');
                    result.push('#');
                    if hex {
                        result.push('x');
                    }
                    result.push_str(&digits);
                    result.push(d);
                    entity_end = false;
                    break;
                }
            }
            if !entity_end && digits.is_empty() {
                // No semicolon found or empty - the for loop ended without ;
                // Continue with the rest of the string
            }
            if digits.is_empty() {
                // No digits before ; - restore
                result.push('\x26');
                result.push('#');
                if hex {
                    result.push('x');
                }
                result.push(';');
                continue;
            }
            // Parse the code point
            let value = if hex {
                u32::from_str_radix(&digits, 16)
            } else {
                digits.parse::<u32>()
            };
            match value {
                Ok(cp) if let Some(ch) = char::from_u32(cp) => {
                    result.push(ch);
                }
                _ => {
                    // Invalid code point - restore the entity
                    result.push('\x26');
                    result.push('#');
                    if hex {
                        result.push('x');
                    }
                    result.push_str(&digits);
                    result.push(';');
                }
            }
        } else {
            result.push(ch);
        }
    }
    result
}
