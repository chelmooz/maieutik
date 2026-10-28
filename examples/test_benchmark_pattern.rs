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

fn benchmark_iteration(n: usize) {
    let dir = tempdir().unwrap();
    let index = open_index(dir.path());
    let docs: Vec<FileContent> = (0..n)
        .map(|i| make_file_content(&format!("Document {}: {} {}", i, FOX, RUST), 1000))
        .collect();
    
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        for (i, doc) in docs.iter().enumerate() {
            index.upsert_file(Path::new(&format!("doc{}.md", i)), doc).await.unwrap();
        }
        index.build_hnsw().await.unwrap();
    });
    // Tuple (dir, index, docs) dropped here - dir first, then index
    println!("Iteration with {} docs completed", n);
}

fn main() {
    // Simulate multiple Criterion iterations with more docs
    for i in 1..=10 {
        println!("Starting iteration {}", i);
        benchmark_iteration(200);
        println!("Completed iteration {}", i);
    }
    println!("All iterations completed successfully!");
}
