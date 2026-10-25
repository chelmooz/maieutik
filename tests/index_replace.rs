//! Remplacement atomique des enregistrements d'un fichier (sans modèle ONNX).

use maieutik::core::indexer::{replace_file_records, VectorRecord};

fn rec(file: &str, i: usize) -> VectorRecord {
    VectorRecord {
        id: format!("{file}#{i}"),
        file_path: file.to_string(),
        chunk_index: i,
        text: format!("{file} chunk {i}"),
        embedding: vec![0.0; 4],
        heading_path: None,
        chunk_ordinal: i,
        display_text: None,
        index_text: None,
    }
}

fn recs(file: &str, n: usize) -> Vec<VectorRecord> {
    (0..n).map(|i| rec(file, i)).collect()
}

fn ids(tree: &sled::Tree) -> Vec<String> {
    tree.iter()
        .map(|r| String::from_utf8(r.unwrap().0.to_vec()).unwrap())
        .collect()
}

fn open_tree(dir: &tempfile::TempDir) -> sled::Tree {
    sled::open(dir.path().join("db")).unwrap().open_tree("vectors").unwrap()
}

#[test]
fn shrinking_file_drops_stale_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let tree = open_tree(&dir);
    replace_file_records(&tree, "a.md", &recs("a.md", 5)).unwrap();
    replace_file_records(&tree, "a.md", &recs("a.md", 3)).unwrap();
    assert_eq!(ids(&tree), vec!["a.md#0", "a.md#1", "a.md#2"]);
}

#[test]
fn growing_file_adds_chunks() {
    let dir = tempfile::tempdir().unwrap();
    let tree = open_tree(&dir);
    replace_file_records(&tree, "a.md", &recs("a.md", 2)).unwrap();
    replace_file_records(&tree, "a.md", &recs("a.md", 4)).unwrap();
    assert_eq!(tree.len(), 4);
}

#[test]
fn empty_records_purge_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let tree = open_tree(&dir);
    replace_file_records(&tree, "a.md", &recs("a.md", 3)).unwrap();
    replace_file_records(&tree, "a.md", &[]).unwrap();
    assert!(tree.is_empty());
}

#[test]
fn other_files_are_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let tree = open_tree(&dir);
    replace_file_records(&tree, "a.md", &recs("a.md", 2)).unwrap();
    replace_file_records(&tree, "b.md", &recs("b.md", 2)).unwrap();
    replace_file_records(&tree, "a.md", &[]).unwrap();
    assert_eq!(ids(&tree), vec!["b.md#0", "b.md#1"]);
}

#[test]
fn hash_in_path_does_not_collide() {
    let dir = tempfile::tempdir().unwrap();
    let tree = open_tree(&dir);
    replace_file_records(&tree, "a", &recs("a", 2)).unwrap();
    replace_file_records(&tree, "a#1.md", &recs("a#1.md", 2)).unwrap();
    replace_file_records(&tree, "a", &[]).unwrap();
    assert_eq!(ids(&tree), vec!["a#1.md#0", "a#1.md#1"]);
    replace_file_records(&tree, "a#1.md", &recs("a#1.md", 1)).unwrap();
    assert_eq!(ids(&tree), vec!["a#1.md#0"]);
}