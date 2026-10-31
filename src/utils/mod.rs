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
    // Decode numeric entities first so `&amp;#65;` is only decoded once.
    let out = decode_numeric_entities(&out);
    // Decode `amp` last to avoid recursively decoding escaped entities.
    out.replace("\x26lt;", "\x3c")
        .replace("\x26gt;", "\x3e")
        .replace("\x26quot;", "\"")
        .replace("\x26apos;", "\'")
        .replace("\x26amp;", "\x26")
}

/// Décode les entités XML numériques : &#123; et &#x1F;.
/// Ne panique pas sur des valeurs invalides ; laisse l'entité telle quelle.
fn decode_numeric_entities(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut offset = 0;
    while offset < s.len() {
        let remaining = &s[offset..];
        if remaining.starts_with('&') {
            if let Some((decoded, consumed)) = parse_numeric_entity(remaining) {
                result.push(decoded);
                offset += consumed;
                continue;
            }
        }
        let ch = remaining.chars().next().expect("offset remains in bounds");
        result.push(ch);
        offset += ch.len_utf8();
    }
    result
}

fn parse_numeric_entity(input: &str) -> Option<(char, usize)> {
    let entity = input.strip_prefix("&#")?;
    let (digits, radix, prefix_len) = if entity.starts_with(['x', 'X']) {
        (&entity[1..], 16, 1)
    } else {
        (entity, 10, 0)
    };
    let end = digits.find(';')?;
    let code_point = u32::from_str_radix(&digits[..end], radix).ok()?;
    let decoded = char::from_u32(code_point)?;
    if decoded.is_control() {
        return None;
    }
    Some((decoded, 2 + prefix_len + end + 1))
}

#[cfg(test)]
mod tests {
    use super::decode_numeric_entities;

    #[test]
    fn malformed_numeric_entities_are_preserved_exactly() {
        let input = "bad &#xZ; and &#65 and \u{000c}a;";
        assert_eq!(decode_numeric_entities(input), input);
    }

    #[test]
    fn valid_numeric_entities_decode_without_control_characters() {
        assert_eq!(
            decode_numeric_entities("&#65; &#x1F642; &#12;"),
            "A 🙂 &#12;"
        );
    }
}
