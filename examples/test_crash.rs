use maieutik::config::FileContent;
use maieutik::core::indexer::{extract_headings, Chunker, HashEmbedder, VectorIndex};
use std::path::Path;
use tempfile::tempdir;

const FOX: &str = "the quick brown fox jumps over the lazy dog near the river";
const RUST: &str = "rust cargo compiler borrow checker lifetimes traits";

fn make_file_content(text: &str, max_chars: usize) -> FileContent {
    let base = FileContent {
        path: Path::new("bench.md").to_path_buf(),
        text: text.to_string(),
        headings: extract_headings(text),
        chunks: Vec::new(),
    };
    let chunks = Chunker::new(max_chars, 0).chunk(&base);
    FileContent { chunks, ..base }
}

fn open_index(dir: &Path) -> VectorIndex {
    VectorIndex::open(dir, HashEmbedder::new()).unwrap()
}

#[tokio::main]
async fn main() {
    let n = 1000usize;
    let dir = tempdir().unwrap();
    let index = open_index(dir.path());
    let docs: Vec<FileContent> = (0..n)
        .map(|i| make_file_content(&format!("Document {}: {} {}", i, FOX, RUST), 1000))
        .collect();
    
    println!("Inserting {} docs...", n);
    for (i, doc) in docs.iter().enumerate() {
        index.upsert_file(Path::new(&format!("doc{}.md", i)), doc).await.unwrap();
    }
    println!("Building HNSW...");
    index.build_hnsw().await.unwrap();
    println!("Searching...");
    let results = index.search("rust cargo compiler", 10).await.unwrap();
    println!("Found {} results", results.len());
    println!("Stats: {:?}", index.stats());
}
