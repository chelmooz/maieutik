//! End-to-end check: real BGE-M3 tokenizer (XLM-RoBERTa, Unigram 250k) → real
//! ONNX BGE-M3 dense embeddings → HNSW search over a controlled corpus.
//!
//! This test refuses to run rather than silently passing when the model, the
//! tokenizer, or the ONNX Runtime shared library is missing. Point it at the
//! files with:
//!
//! ```text
//! MAIEUTIK_EMBEDDER_MODEL=/path/to/model.onnx
//! MAIEUTIK_EMBEDDER_TOKENIZER=/path/to/tokenizer.json
//! ORT_DYLIB_PATH=/path/to/libonnxruntime.so
//! ```

use std::path::{Path, PathBuf};

use maieutik::config::FileContent;
use maieutik::core::indexer::{extract_headings, Chunker, Embedder, VectorIndex};

/// Chunk size used for the corpus. Chosen so each document yields two chunks.
const MAX_CHUNK_CHARS: usize = 220;

/// A relevant match must be closer than this. Embeddings are L2-normalized, so
/// Euclidean distance is `sqrt(2 - 2*cos)`: 0.0 is identical, ~1.41 orthogonal.
/// Measured top-1 distances with BGE-M3 on this corpus: 0.87, 0.78, 0.70 —
/// while the best match from a *wrong* file starts at 1.09. 0.95 sits between
/// the two with margin on each side (0.08 above the worst right answer, 0.14
/// below the best wrong answer). A value below 0.87 fails by design on the
/// first query (verified: 0.80 rejects the 0.8670 match).
const MAX_MATCH_DISTANCE: f32 = 0.95;

/// The best result from another file must be at least this much farther away
/// than the correct one. Measured margins with BGE-M3: 0.23, 0.40, 0.39.
const MIN_FILE_MARGIN: f32 = 0.15;

fn require_file(var: &str, default: fn() -> PathBuf, what: &str) -> PathBuf {
    let path = std::env::var_os(var)
        .map(PathBuf::from)
        .unwrap_or_else(default);
    assert!(
        path.is_file(),
        "\n{what} not found at {}\n\
         This test needs the real BAAI/bge-m3 files; it will not pass without them.\n\
         Set {var}=/path/to/{what} (download from https://huggingface.co/BAAI/bge-m3,\n\
         onnx/{what} and tokenizer.json).\n",
        path.display(),
    );
    path
}

fn build_index(dir: &Path) -> VectorIndex {
    let model = require_file(
        "MAIEUTIK_EMBEDDER_MODEL",
        maieutik::config::store::embedder_model_path,
        "model.onnx",
    );
    let tokenizer = require_file(
        "MAIEUTIK_EMBEDDER_TOKENIZER",
        maieutik::config::store::embedder_tokenizer_path,
        "tokenizer.json",
    );

    let embedder = Embedder::new(&model, &tokenizer).unwrap_or_else(|e| {
        panic!(
            "\nfailed to load the ONNX embedder: {e:#}\n\
             If ONNX Runtime is missing, set ORT_DYLIB_PATH to a libonnxruntime.so\n\
             matching the `ort` API level (>= 1.31).\n"
        )
    });

    VectorIndex::open(dir, embedder).expect("open vector index")
}

fn document(path: &str, text: &str) -> (PathBuf, FileContent) {
    let headings = extract_headings(text);
    let chunker = Chunker::new(MAX_CHUNK_CHARS, 0);
    let chunks = chunker.chunk(&FileContent {
        path: PathBuf::from(path),
        text: text.to_string(),
        headings: headings.clone(),
        chunks: Vec::new(),
    });
    (
        PathBuf::from(path),
        FileContent {
            path: PathBuf::from(path),
            text: text.to_string(),
            headings,
            chunks,
        },
    )
}

#[tokio::test]
#[ignore = "requires the BGE-M3 model files (MAIEUTIK_EMBEDDER_MODEL/TOKENIZER) and libonnxruntime; run with --ignored"]
async fn indexes_and_retrieves_the_right_chunk() {
    let dir = tempfile::tempdir().expect("temp dir");
    let index = build_index(dir.path());

    let corpus = [
        (
            "rust.md",
            "# Ownership\n\n\
             Rust enforces memory safety at compile time without a garbage collector. \
             Every value has exactly one owner, and the value is dropped when that owner \
             leaves scope.\n\n\
             # Borrowing\n\n\
             The borrow checker rejects data races before the program runs. You may hold \
             many immutable references or one mutable reference, but never both at once.\n",
        ),
        (
            "bread.md",
            "# Sourdough\n\n\
             A sourdough starter is a culture of wild yeast and lactic acid bacteria. \
             Feeding it flour and water every twelve hours keeps the culture active.\n\n\
             # Fermentation\n\n\
             Dough rises for four to eight hours at room temperature. A slow cold retard \
             in the fridge develops a deeper, more sour flavour.\n",
        ),
        (
            "postgres.md",
            "# Indexes\n\n\
             A B-tree index keeps keys sorted so the database can find rows without \
             scanning the whole table. Lookups become logarithmic instead of linear.\n\n\
             # Query planning\n\n\
             The planner compares sequential scans with index scans using table \
             statistics. Stale statistics lead to bad plans and slow queries.\n",
        ),
    ];

    let mut expected: Vec<(String, Vec<String>)> = Vec::new();
    for (name, text) in corpus {
        let (path, content) = document(name, text);
        assert!(
            content.chunks.len() >= 2,
            "{name} should produce at least two chunks, got {}",
            content.chunks.len()
        );
        index.upsert_file(&path, &content).await.expect("upsert");
        expected.push((
            name.to_string(),
            content.chunks.iter().map(|c| c.text.clone()).collect(),
        ));
    }

    let cases = [
        (
            "how does rust prevent data races without a garbage collector",
            "rust.md",
            "borrow checker",
        ),
        (
            "does a cold retard make sourdough bread more sour",
            "bread.md",
            "cold retard",
        ),
        (
            "why do stale statistics cause slow queries",
            "postgres.md",
            "Stale statistics",
        ),
    ];

    for (query, want_file, want_phrase) in cases {
        let results = index.search(query, 3).await.expect("search");
        assert!(
            !results.is_empty(),
            "search returned no results for {query:?}"
        );

        println!("\nquery: {query:?}");
        for (rank, (file, text, distance)) in results.iter().enumerate() {
            println!(
                "  #{rank} {file}  d={distance:.4}  {:?}",
                text.chars().take(60).collect::<String>()
            );
        }

        let (best_file, best_text, best_distance) = &results[0];
        assert!(
            best_file.ends_with(want_file),
            "query {query:?} matched {best_file}, expected {want_file}"
        );
        assert!(
            best_text.contains(want_phrase),
            "query {query:?} matched the wrong chunk of {want_file}: expected the chunk \
             containing {want_phrase:?}, got {:?}",
            best_text.chars().take(120).collect::<String>()
        );
        assert!(
            best_distance.is_finite() && *best_distance < MAX_MATCH_DISTANCE,
            "query {query:?} scored d={best_distance:.4}, above the {MAX_MATCH_DISTANCE} threshold"
        );

        // A correct file must also beat every other file by a clear margin,
        // otherwise the ranking is accidental.
        if let Some((other_file, _, other_distance)) = results
            .iter()
            .skip(1)
            .find(|(file, _, _)| !file.ends_with(want_file))
        {
            assert!(
                other_distance - best_distance >= MIN_FILE_MARGIN,
                "query {query:?}: {want_file} (d={best_distance:.4}) does not beat \
                 {other_file} (d={other_distance:.4}) by the {MIN_FILE_MARGIN} margin"
            );
        }

        // The returned text must be one of the chunks actually indexed.
        let (_, chunks) = expected
            .iter()
            .find(|(name, _)| name == want_file)
            .expect("corpus entry");
        assert!(
            chunks.iter().any(|chunk| chunk == best_text),
            "query {query:?} returned text that is not an indexed chunk of {want_file}"
        );
    }
}

#[test]
#[ignore = "loads the 2.27 GB BGE-M3 twice (~4.5 GB RAM); run explicitly with --ignored"]
fn two_embedders_share_one_ort_environment() {
    // Regression: ONNX Runtime allows a single environment per process, so
    // constructing a second `Embedder` must reuse the process-wide one instead
    // of calling `ort::init()` again.
    let model = require_file(
        "MAIEUTIK_EMBEDDER_MODEL",
        maieutik::config::store::embedder_model_path,
        "model.onnx",
    );
    let tokenizer = require_file(
        "MAIEUTIK_EMBEDDER_TOKENIZER",
        maieutik::config::store::embedder_tokenizer_path,
        "tokenizer.json",
    );

    let first = Embedder::new(&model, &tokenizer).expect("first embedder");
    let second = Embedder::new(&model, &tokenizer).expect("second embedder");

    assert_eq!(
        first.embed("shared environment").unwrap(),
        second.embed("shared environment").unwrap(),
    );
}

#[test]
#[ignore = "requires the BGE-M3 model files (MAIEUTIK_EMBEDDER_MODEL/TOKENIZER) and libonnxruntime; run with --ignored"]
fn embeddings_are_normalized_and_deterministic() {
    let model = require_file(
        "MAIEUTIK_EMBEDDER_MODEL",
        maieutik::config::store::embedder_model_path,
        "model.onnx",
    );
    let tokenizer = require_file(
        "MAIEUTIK_EMBEDDER_TOKENIZER",
        maieutik::config::store::embedder_tokenizer_path,
        "tokenizer.json",
    );
    let embedder = Embedder::new(&model, &tokenizer).unwrap_or_else(|e| {
        panic!(
            "\nfailed to load the ONNX embedder: {e:#}\n\
             If ONNX Runtime is missing, set ORT_DYLIB_PATH to a libonnxruntime.so \
             matching the `ort` API level (>= 1.31).\n"
        )
    });

    let a = embedder.embed("memory safety without a garbage collector").unwrap();
    let b = embedder.embed("memory safety without a garbage collector").unwrap();

    assert_eq!(a.len(), 1024, "BGE-M3 must produce 1024 dimensions");
    assert_eq!(a, b, "embeddings must be deterministic");

    let norm: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    assert!(
        (norm - 1.0).abs() < 1e-4,
        "embedding must be L2-normalized, got norm {norm}"
    );

    // Different meanings must not collapse to the same vector.
    let other = embedder.embed("sourdough starter fermentation").unwrap();
    let cosine: f32 = a.iter().zip(&other).map(|(x, y)| x * y).sum();
    assert!(
        cosine < 0.9,
        "unrelated sentences should not be near-identical (cosine {cosine})"
    );
}
