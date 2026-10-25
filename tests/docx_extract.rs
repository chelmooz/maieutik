//! Extraction DOCX minimale (sans modèle ONNX).

use maieutik::utils::extract_text;
use std::io::Write;
use std::path::PathBuf;

fn nl() -> String {
    String::from(char::from(10u8))
}

fn make_zip(dir: &tempfile::TempDir, name: &str, entries: &[(&str, &str)]) -> PathBuf {
    let path = dir.path().join(name);
    let file = std::fs::File::create(&path).unwrap();
    let mut writer = zip::ZipWriter::new(file);
    for (entry, body) in entries {
        writer
            .start_file(*entry, zip::write::FileOptions::default())
            .unwrap();
        writer.write_all(body.as_bytes()).unwrap();
    }
    writer.finish().unwrap();
    path
}

#[test]
fn entities_are_decoded_once() {
    let dir = tempfile::tempdir().unwrap();
    let xml = "<w:document><w:body><w:p><w:r><w:t>x \x26lt; y \x26amp;lt; z \x26quot;q\x26quot; \x26apos;s\x26apos; \x26gt;</w:t></w:r></w:p></w:body></w:document>";
    let path = make_zip(&dir, "a.docx", &[("word/document.xml", xml)]);
    let expected = format!("x \x3c y \x26lt; z \"q\" 's' \x3e{}", nl());
    assert_eq!(extract_text(&path).unwrap(), expected);
}

#[test]
fn paragraphs_are_separated_by_newline() {
    let dir = tempfile::tempdir().unwrap();
    let xml = "<w:document><w:body><w:p><w:r><w:t>A</w:t></w:r></w:p><w:p><w:r><w:t>B</w:t></w:r></w:p></w:body></w:document>";
    let path = make_zip(&dir, "a.docx", &[("word/document.xml", xml)]);
    let expected = format!("A{}B{}", nl(), nl());
    assert_eq!(extract_text(&path).unwrap(), expected);
}

#[test]
fn missing_document_xml_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = make_zip(&dir, "a.docx", &[("other.xml", "x")]);
    assert!(extract_text(&path).is_err());
}

#[test]
fn not_a_zip_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("b.docx");
    std::fs::write(&path, "pas un zip").unwrap();
    assert!(extract_text(&path).is_err());
}

#[test]
fn plain_text_still_works() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("c.md");
    std::fs::write(&path, "# T").unwrap();
    assert_eq!(extract_text(&path).unwrap(), "# T");
}