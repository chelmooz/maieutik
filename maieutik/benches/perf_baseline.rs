//! Performance baseline for Maieutik — run with `cargo bench --bench perf_baseline`.
//!
//! Mesure : indexation, recherche, mémoire, taille d'index.
//! Exécuté sans modèle ONNX (HashEmbedder déterministe) pour la reproductibilité.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
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

fn bench_indexing_10(c: &mut Criterion) {
    let n = 10usize;
    c.bench_function("indexing_10_docs", |b| {
        b.iter_batched(
            || {
                let dir = tempdir().unwrap();
                let index = open_index(dir.path());
                let docs: Vec<FileContent> = (0..n)
                    .map(|i| make_file_content(&format!("Document {}: {} {}", i, FOX, RUST), 1000))
                    .collect();
                (dir, index, docs)
            },
            |(_, index, docs)| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    for (i, doc) in docs.iter().enumerate() {
                        index
                            .store_file(Path::new(&format!("doc{}.md", i)), doc, None)
                            .unwrap();
                    }
                    index.build_hnsw().await.unwrap();
                });
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_indexing_100(c: &mut Criterion) {
    let n = 100usize;
    c.bench_function("indexing_100_docs", |b| {
        b.iter_batched(
            || {
                let dir = tempdir().unwrap();
                let index = open_index(dir.path());
                let docs: Vec<FileContent> = (0..n)
                    .map(|i| make_file_content(&format!("Document {}: {} {}", i, FOX, RUST), 1000))
                    .collect();
                (dir, index, docs)
            },
            |(_, index, docs)| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    for (i, doc) in docs.iter().enumerate() {
                        index
                            .store_file(Path::new(&format!("doc{}.md", i)), doc, None)
                            .unwrap();
                    }
                    index.build_hnsw().await.unwrap();
                });
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_indexing_1000(c: &mut Criterion) {
    let n = 1000usize;
    c.bench_function("indexing_1000_docs", |b| {
        b.iter_batched(
            || {
                let dir = tempdir().unwrap();
                let index = open_index(dir.path());
                let docs: Vec<FileContent> = (0..n)
                    .map(|i| make_file_content(&format!("Document {}: {} {}", i, FOX, RUST), 1000))
                    .collect();
                (dir, index, docs)
            },
            |(_, index, docs)| {
                let rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async {
                    for (i, doc) in docs.iter().enumerate() {
                        index
                            .store_file(Path::new(&format!("doc{}.md", i)), doc, None)
                            .unwrap();
                    }
                    index.build_hnsw().await.unwrap();
                });
            },
            criterion::BatchSize::SmallInput,
        );
    });
}

fn bench_search_1(c: &mut Criterion) {
    let k = 1usize;
    let dir = tempdir().unwrap();
    let index = open_index(dir.path());
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        for i in 0..1000 {
            index
                .store_file(
                    Path::new(&format!("doc{}.md", i)),
                    &make_file_content(&format!("Document {}: {} {}", i, FOX, RUST), 1000),
                    None,
                )
                .unwrap();
        }
        index.build_hnsw().await.unwrap();
    });
    c.bench_function("search_top_1", |b| {
        b.iter(|| {
            rt.block_on(async {
                black_box(index.search("rust cargo compiler", k).await.unwrap());
            });
        });
    });
}

fn bench_search_5(c: &mut Criterion) {
    let k = 5usize;
    let dir = tempdir().unwrap();
    let index = open_index(dir.path());
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        for i in 0..1000 {
            index
                .store_file(
                    Path::new(&format!("doc{}.md", i)),
                    &make_file_content(&format!("Document {}: {} {}", i, FOX, RUST), 1000),
                    None,
                )
                .unwrap();
        }
        index.build_hnsw().await.unwrap();
    });
    c.bench_function("search_top_5", |b| {
        b.iter(|| {
            rt.block_on(async {
                black_box(index.search("rust cargo compiler", k).await.unwrap());
            });
        });
    });
}

fn bench_search_10(c: &mut Criterion) {
    let k = 10usize;
    let dir = tempdir().unwrap();
    let index = open_index(dir.path());
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        for i in 0..1000 {
            index
                .store_file(
                    Path::new(&format!("doc{}.md", i)),
                    &make_file_content(&format!("Document {}: {} {}", i, FOX, RUST), 1000),
                    None,
                )
                .unwrap();
        }
        index.build_hnsw().await.unwrap();
    });
    c.bench_function("search_top_10", |b| {
        b.iter(|| {
            rt.block_on(async {
                black_box(index.search("rust cargo compiler", k).await.unwrap());
            });
        });
    });
}

fn bench_search_50(c: &mut Criterion) {
    let k = 50usize;
    let dir = tempdir().unwrap();
    let index = open_index(dir.path());
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        for i in 0..1000 {
            index
                .store_file(
                    Path::new(&format!("doc{}.md", i)),
                    &make_file_content(&format!("Document {}: {} {}", i, FOX, RUST), 1000),
                    None,
                )
                .unwrap();
        }
        index.build_hnsw().await.unwrap();
    });
    c.bench_function("search_top_50", |b| {
        b.iter(|| {
            rt.block_on(async {
                black_box(index.search("rust cargo compiler", k).await.unwrap());
            });
        });
    });
}

fn bench_memory(c: &mut Criterion) {
    c.bench_function("index_size_1000_docs", |b| {
        let dir = tempdir().unwrap();
        let index = open_index(dir.path());
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            for i in 0..1000 {
                index
                    .store_file(
                        Path::new(&format!("doc{}.md", i)),
                        &make_file_content(&format!("Document {}: {} {}", i, FOX, RUST), 1000),
                        None,
                    )
                    .unwrap();
            }
            index.build_hnsw().await.unwrap();
        });
        b.iter(|| {
            let (total_chunks, index_size) = index.stats();
            black_box((total_chunks, index_size));
        });
    });
}

criterion_group!(
    benches,
    bench_indexing_10,
    bench_indexing_100,
    bench_indexing_1000,
    bench_search_1,
    bench_search_5,
    bench_search_10,
    bench_search_50,
    bench_memory
);
criterion_main!(benches);
