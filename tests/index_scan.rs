//! Parcours et indexation par lot sans modèle ONNX (HashEmbedder).

use maieutik::config::AppSettings;
use maieutik::core::indexer::{HashEmbedder, VectorIndex};
use maieutik::core::scan::{collect_files, index_all};
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::RwLock;

const FOX: &str = "the quick brown fox jumps over the lazy dog near the river";
const RUST: &str = "rust cargo compiler borrow checker lifetimes traits";
const BREAD: &str = "bread flour yeast oven baking sourdough starter";

fn open(dir: &Path) -> VectorIndex {
    let settings = Arc::new(RwLock::new(AppSettings::default()));
    VectorIndex::open(dir, settings, HashEmbedder::new()).unwrap()
}

fn settings_for(folder: &Path) -> AppSettings {
    AppSettings {
        folders: vec![folder.to_string_lossy().to_string()],
        ..AppSettings::default()
    }
}

fn write(dir: &Path, name: &str, text: &str) {
    std::fs::write(dir.join(name), text).unwrap();
}

fn write_docx(dir: &Path, name: &str, text: &str) {
    let file = std::fs::File::create(dir.join(name)).unwrap();
    let mut zip = zip::ZipWriter::new(file);
    zip.start_file("word/document.xml", zip::write::FileOptions::default())
        .unwrap();
    let xml = format!("<w:document><w:body><w:p><w:r><w:t>{text}</w:t></w:r></w:p></w:body></w:document>");
    zip.write_all(xml.as_bytes()).unwrap();
    zip.finish().unwrap();
}

#[test]
fn collect_files_applies_extension_filter_and_skips_symlinks() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.md", FOX);
    write(dir.path(), "b.txt", RUST);
    write(dir.path(), "D.MD", BREAD);
    write(dir.path(), "skip.rs", "fn main() {}");
    std::fs::create_dir(dir.path().join("sub")).unwrap();
    write(&dir.path().join("sub"), "c.md", FOX);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(dir.path().join("a.md"), dir.path().join("link.md")).unwrap();
    }
    let (files, errors) = collect_files(&settings_for(dir.path()));
    assert!(errors.is_empty());
    let mut names: Vec<String> = files
        .iter()
        .map(|p| p.file_name().unwrap().to_string_lossy().to_string())
        .collect();
    names.sort();
    assert_eq!(names, vec!["D.MD", "a.md", "b.txt", "c.md"]);
}

#[test]
fn collect_files_respects_size_limit() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.md", FOX);
    let settings = AppSettings {
        max_file_size: 0,
        ..settings_for(dir.path())
    };
    let (files, errors) = collect_files(&settings);
    assert!(errors.is_empty());
    assert!(files.is_empty());
}

#[test]
fn collect_files_reports_missing_folder() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("absent");
    let (files, errors) = collect_files(&settings_for(&missing));
    assert!(files.is_empty());
    assert_eq!(errors.len(), 1);
}

#[tokio::test]
async fn index_all_indexes_md_txt_and_docx_then_searches() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    write(docs.path(), "a.md", FOX);
    write(docs.path(), "b.md", RUST);
    write_docx(docs.path(), "c.docx", BREAD);
    let index = open(data.path());
    let report = index_all(&index, &settings_for(docs.path()), false)
        .await
        .unwrap();
    assert_eq!(report.indexed, 3);
    assert!(report.failed.is_empty());
    assert_eq!(report.exit_code(), 0);
    let hits = index.search("rust borrow checker", 3).await.unwrap();
    assert!(hits[0].0.ends_with("b.md"));
    let hits = index.search("sourdough starter", 3).await.unwrap();
    assert!(hits[0].0.ends_with("c.docx"));
}

#[tokio::test]
async fn failed_file_is_reported_and_keeps_its_old_records() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    write_docx(docs.path(), "good.docx", BREAD);
    let index = open(data.path());
    let settings = settings_for(docs.path());
    let first = index_all(&index, &settings, false).await.unwrap();
    assert_eq!(first.indexed, 1);
    assert_eq!(index.stats().0, 1);
    write(docs.path(), "good.docx", "not a zip");
    let second = index_all(&index, &settings, false).await.unwrap();
    assert_eq!(second.indexed, 0);
    assert_eq!(second.failed.len(), 1);
    assert_eq!(second.exit_code(), 1);
    assert_eq!(index.stats().0, 1);
}

#[tokio::test]
async fn emptied_file_is_purged() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    write(docs.path(), "a.md", FOX);
    let index = open(data.path());
    let settings = settings_for(docs.path());
    index_all(&index, &settings, false).await.unwrap();
    assert_eq!(index.stats().0, 1);
    write(docs.path(), "a.md", "");
    let report = index_all(&index, &settings, false).await.unwrap();
    assert_eq!(report.indexed, 1);
    assert_eq!(index.stats().0, 0);
}

#[tokio::test]
async fn force_removes_vanished_files_but_plain_run_keeps_them() {
    let data = tempfile::tempdir().unwrap();
    let docs = tempfile::tempdir().unwrap();
    write(docs.path(), "a.md", FOX);
    write(docs.path(), "b.md", RUST);
    let index = open(data.path());
    let settings = settings_for(docs.path());
    index_all(&index, &settings, false).await.unwrap();
    assert_eq!(index.stats().0, 2);
    std::fs::remove_file(docs.path().join("b.md")).unwrap();
    index_all(&index, &settings, false).await.unwrap();
    assert_eq!(index.stats().0, 2);
    index_all(&index, &settings, true).await.unwrap();
    assert_eq!(index.stats().0, 1);
}