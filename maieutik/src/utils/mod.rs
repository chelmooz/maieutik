//! Utilities — document extraction, hashing

use anyhow::Result;
use std::io::Read;
use std::path::Path;

pub fn parse_duration(s: &str) -> Option<u64> {
    s.trim().parse::<u64>().ok()
}

pub fn format_timestamp(ts: u64) -> String {
    format!("{ts}")
}

pub fn escape_shell_chars(s: &str) -> String {
    s.replace('\'', "'\\''")
}

pub fn sanitize_path(p: &str) -> String {
    p.chars()
        .filter(|c| c.is_alphanumeric() || *c == '/' || *c == '.' || *c == '_' || *c == '-')
        .collect()
}

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
    let doc = lopdf::Document::load(path)?;
    let page_count = doc.get_pages().len();
    Ok(format!("PDF with {} pages", page_count))
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
    // Decode numeric entities first so `&#65;` is only decoded once.
    let out = decode_numeric_entities(&out);
    // Decode `amp` last to avoid recursively decoding escaped entities.
    out.replace("\x26lt;", "\x3c")
        .replace("\x26gt;", "\x3e")
        .replace("\x26quot;", "\"")
        .replace("\x26apos;", "\'")
        .replace("\x26amp;", "\x26")
}

/// Décode les entités XML numériques (`&#123;`, `&#x1F;`) en une seule passe.
/// - Entités malformées (syntaxe, point de code hors plage, surrogates) : conservées telles quelles.
/// - Ordre de décodage : numérique d'abord, puis entités nommées (`<`, `>`, `"`, `&apos;`, `&`).
///   Ainsi `&#65;` devient `&#65;` (via `&` → `&`) mais n'est **pas** re-décodé en `A`.
fn decode_numeric_entities(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'&' && i + 2 < bytes.len() && bytes[i + 1] == b'#' {
            // Tentative de lecture d'une entité numérique : &#...; ou &#x...;
            let start = i;
            i += 2; // skip "&#"
            let is_hex = bytes.get(i) == Some(&b'x') || bytes.get(i) == Some(&b'X');
            if is_hex {
                i += 1;
            }
            let digit_start = i;
            // Lire les chiffres (max 7 pour U+10FFFF = 1114111, +1 pour sécurité)
            while i < bytes.len() && i - digit_start < 8 && bytes[i].is_ascii_hexdigit() {
                i += 1;
            }
            // Doit y avoir au moins un chiffre et un ';' final
            if i < bytes.len() && bytes[i] == b';' && i > digit_start {
                let digits = &bytes[digit_start..i];
                let radix = if is_hex { 16 } else { 10 };
                if let Ok(code_point) =
                    u32::from_str_radix(std::str::from_utf8(digits).unwrap_or(""), radix)
                {
                    // Valider : plage Unicode valide, pas de surrogates, pas de contrôle
                    if code_point <= 0x10FFFF
                        && !(0xD800..=0xDFFF).contains(&code_point)
                        && !is_control_codepoint(code_point)
                    {
                        if let Some(ch) = char::from_u32(code_point) {
                            result.push(ch);
                            i += 1; // skip ';'
                            continue;
                        }
                    }
                }
            }
            // Entité invalide : restaurer le texte original depuis '&'
            result.push_str(std::str::from_utf8(&bytes[start..i]).unwrap_or(""));
            continue;
        }
        // Caractère normal : avancer d'un codepoint UTF-8
        let ch = s[i..]
            .chars()
            .next()
            .expect("UTF-8 slice should yield a character");
        result.push(ch);
        i += ch.len_utf8();
    }
    result
}

/// Vérifie si un point de code Unicode est un caractère de contrôle (Cc ou Cf).
/// Les surrogates (0xD800–0xDFFF) sont exclus par l'appelant.
fn is_control_codepoint(cp: u32) -> bool {
    matches!(cp, 0x00..=0x1F | 0x7F..=0x9F)
}

#[cfg(test)]
mod tests {
    use super::decode_numeric_entities;

    #[test]
    fn malformed_numeric_entities_are_preserved_exactly() {
        // Syntaxe invalide : &#xZ; (hex invalide), &#65 (point-virgule manquant), &#; (vide)
        let input = "bad &#xZ; and &#65 and &#;";
        assert_eq!(decode_numeric_entities(input), input);
    }

    #[test]
    fn valid_numeric_entities_decode_without_control_characters() {
        // &#65; = 'A', &#x1F642; = '🙂', &#12; = contrôle (FF) → préservé
        assert_eq!(
            decode_numeric_entities("&#65; &#x1F642; &#12;"),
            "A 🙂 &#12;"
        );
    }

    #[test]
    fn amp_escaped_numeric_not_double_decoded() {
        // &#65; (ampersand échappé + entité numérique) :
        // 1. decode_numeric_entities ne touche pas (pas de &# au début)
        // 2. replace & → & donne &#65; littéral, PAS 'A'
        let input = "\x26amp;\x2365;"; // "&#65;"
        let after_numeric = decode_numeric_entities(input);
        assert_eq!(after_numeric, input); // inchangé car pas de &#
        let final_result = after_numeric
            .replace("\x26lt;", "\x3c")
            .replace("\x26gt;", "\x3e")
            .replace("\x26quot;", "\"")
            .replace("\x26apos;", "\'")
            .replace("\x26amp;", "\x26");
        assert_eq!(final_result, "&#65;"); // littéral, PAS 'A'
    }

    #[test]
    fn raw_numeric_entity_is_decoded_once() {
        // &#65; brut (non échappé) → décodé en 'A' au premier passage
        let input = "&#65;";
        let after_numeric = decode_numeric_entities(input);
        assert_eq!(after_numeric, "A");
        // Les remplacements nommés ne changent rien (pas d'entités nommées)
        let final_result = after_numeric
            .replace("\x26lt;", "\x3c")
            .replace("\x26gt;", "\x3e")
            .replace("\x26quot;", "\"")
            .replace("\x26apos;", "\'")
            .replace("\x26amp;", "\x26");
        assert_eq!(final_result, "A");
    }

    #[test]
    fn surrogate_codepoints_are_rejected() {
        // &#xD800; (surrogate haut) et &#xDFFF; (surrogate bas) → préservés
        assert_eq!(decode_numeric_entities("&#xD800;"), "&#xD800;");
        assert_eq!(decode_numeric_entities("&#xDFFF;"), "&#xDFFF;");
    }

    #[test]
    fn out_of_range_codepoints_are_rejected() {
        // &#1114112; = 0x110000 > 0x10FFFF → préservé
        assert_eq!(decode_numeric_entities("&#1114112;"), "&#1114112;");
    }

    #[test]
    fn mixed_valid_and_invalid_entities() {
        let input = "A &#65; B &#xZZ; C &#x1F642; D";
        assert_eq!(decode_numeric_entities(input), "A A B &#xZZ; C 🙂 D");
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(super::parse_duration("123"), Some(123));
        assert_eq!(super::parse_duration("abc"), None);
    }

    #[test]
    fn test_format_timestamp() {
        assert_eq!(super::format_timestamp(12345), "12345");
    }

    #[test]
    fn test_escape_shell_chars() {
        assert_eq!(super::escape_shell_chars("it's"), "it'\\''s");
    }

    #[test]
    fn test_sanitize_path() {
        assert_eq!(super::sanitize_path("a/b$c"), "a/bc");
    }

    #[test]
    fn test_strip_xml_tags_basic() {
        let xml = "<w:p>Hello</w:p><w:p>World</w:p>";
        let out = super::strip_xml_tags(xml);
        assert!(out.contains("Hello"));
        assert!(out.contains("World"));
    }

    #[test]
    fn test_decode_numeric_entities_valid() {
        let s = "A &#65; B";
        assert_eq!(super::decode_numeric_entities(s), "A A B");
    }
}
