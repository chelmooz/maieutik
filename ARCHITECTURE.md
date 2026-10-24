# Architecture — Maieutik

Local-first notes with AI: index your notes, retrieve them semantically,
rerank, generate sourced answers, judge the answers. Everything runs locally;
every external component sits behind a Rust trait and is selected by
configuration, never by code.

## Component table (real status, evidence-backed)

| Component | Trait / module | Status | Evidence |
|---|---|---|---|
| ORT environment | `core::ort_env` | **tested** — shared process-wide environment, two sessions coexist | `tests/embedding_search.rs::two_embedders_share_one_ort_environment` (passes with the real model, see `VERIFICATION.md`) |
| Embedder (BGE-M3 dense) | `core::indexer::Embedder` | **tested** — real ONNX model, 1024-dim, distances logged | same test file; `test-model-nocapture.log` |
| Vector store + search | `core::indexer::VectorIndex` (sled + fast-hnsw) | **tested at small scale**; known limits: full HNSW rebuild per upsert, stale chunks kept when a file shrinks | `src/core/indexer/mod.rs` (`rebuild_hnsw_if_needed`, `upsert_file`) |
| Chunker | `core::indexer::Chunker` | **broken** — panics on non-ASCII text with overlap > 0; heading offsets wrong on CRLF | `VERIFICATION.md` issue 2; probe output in the review archive |
| Headings extraction | `core::indexer::extract_headings` | **broken on CRLF** (same issue) | same |
| File watcher + debounce | `core::watch` | **stub** — code exists, never exercised by a test, pipeline not wired | `src/core/watch.rs`; no test references it |
| Extraction (docx/pdf/txt) | `utils` | **partial/broken** — XML entities not decoded (`strip_xml_tags`), PDF parsing can panic | `src/utils/mod.rs:44` ; audit probe output |
| LLM generation (llama.cpp) | `llm` | **never executed** — no model ever loaded; sampler chain incomplete; 512-token batch cap | `VERIFICATION.md` issue 6 |
| OpenAI-compatible provider | `llm` | **never executed** — no timeout, no status check, no true streaming | same |
| MCP / JSON-RPC bridge | `mcp` | **stub** — handlers defined, not wired to index/LLM; socket path uses caller PID | `src/mcp/mod.rs` (`socket_path`) |
| Speech (VAD/STT) | `speech` | **stub, unverified** — Silero VAD input shapes never checked against the real model file | `VERIFICATION.md` issue 5 |
| Telemetry | — | **removed** | git history (pre-publication cleanup); no module in `src/` |
| GUI | `ui` | **stub** — headless build, logs a warning | `src/ui/mod.rs:12` |

Status vocabulary: **tested** = a real automated run with raw output exists;
**stub** = compiles but does nothing or is not wired; **broken** = compiles but
a documented defect makes it unusable on real input; **never executed** = no
run exists at all.

## Traits (the seams — P3 and later)

One trait per external component; adapters implement them; configuration
selects the implementation. Following contract-first: since every participant
shares the same build, the typed interfaces below ARE the contract — no
OpenAPI machinery for a single-crate boundary.

```rust
/// Dense vector source. Adapter today: BGE-M3 via ONNX (tested).
trait Embedder: Send + Sync {
    fn embed(&self, text: &str) -> Result<Vec<f32>>;   // L2-normalized
    fn dim(&self) -> usize;
    fn model_id(&self) -> &str;
}

/// Retrieval over the index. Adapter today: sled + HNSW dense search.
trait Retriever: Send + Sync {
    fn search(&self, query: &str, k: usize) -> Result<Vec<Hit>>;
    // Hit { chunk_id, file_path, ordinal, heading_path, text, distance, cosine }
}

/// Cross-encoder rescoring (P4). Adapter planned: BGE reranker via ONNX.
trait Reranker: Send + Sync {
    fn score_pairs(&self, query: &str, passages: &[String]) -> Result<Vec<f32>>;
}

/// Query transformation (P6): multi-query, HyDE. Default adapter: identity.
trait QueryExpander: Send + Sync {
    fn expand(&self, query: &str) -> Result<Vec<String>>;
}

/// Answer generation (P5). Adapters: llama.cpp first; OpenAI-compatible optional.
trait Generator: Send + Sync {
    fn generate(&self, prompt: Prompt) -> Result<Answer>;
    // Answer { text, sources: Vec<ChunkRef> }
}

/// Answer support judgement (P7). Adapter planned: Laya sidecar (HTTP).
trait Judge: Send + Sync {
    fn judge(&self, question: &str, passages: &[String], answer: &str)
        -> Result<Judgement>;
    // Judgement { verdict, confidence, source }
    // Error::Unavailable when the sidecar is absent — never a default success.
}

/// Speech-to-text (P8). Adapters: external OpenWhispr (A) or sherpa-onnx (B).
trait Transcriber: Send + Sync {
    fn transcribe(&self, audio: &std::path::Path) -> Result<String>;
}
```

Error policy: recoverable component failures return typed errors; an optional
component that is not configured returns `Error::Unavailable` — callers surface
it, they never substitute a default success.

## Pipeline (target, with today's wiring)

```
fichier
  → extraction (utils)                    [stub/broken — P3]
  → chunk (Chunker)                       [broken — P3]
  → embed (Embedder)                      [tested]
  → index (VectorIndex: sled + HNSW)      [tested at small scale]
  → recherche dense (Retriever)           [tested at small scale]
  → recherche lexicale (FTS5|tantivy)     [à faire — décision ouverte]
  → fusion RRF                            [à faire]
  → rerank (Reranker)                     [à faire — P4]
  → génération (Generator + sources)      [never executed — P5]
  → jugement (Judge)                      [à faire — P7]
```

**Wired today**: nothing end-to-end. The watcher → extraction → chunk → index
chain is not connected (`run_watcher` leaves re-indexing unimplemented,
`Commands::Index` is an empty stub), and nothing after the index exists. The only complete path is the test path:
corpus → chunk → embed → index → dense search, exercised by
`tests/embedding_search.rs`.

## Chunk metadata (P3)

Each stored chunk will carry, in addition to text and embedding:

| Field | Meaning |
|---|---|
| `source` | file path, byte range, heading path |
| `provenance` | extractor + version, chunker version, embedding model id |
| `confiance` | extraction quality marker (e.g. decode fallback used, truncation happened) |

Rationale: answers must be attributable to a provenance chain, and degraded
extractions must be visible instead of silently polluting retrieval.

## Configuration (MAIEUTIK_* variables)

| Variable | Component | Phase |
|---|---|---|
| `MAIEUTIK_EMBEDDER_MODEL` / `_TOKENIZER` | Embedder | in place |
| `MAIEUTIK_EMBEDDER_THREADS` | Embedder | in place |
| `ORT_DYLIB_PATH` | ORT loader | in place |
| `MAIEUTIK_DOWNLOAD_MODELS` | build-time downloader (off by default) | in place |
| `MAIEUTIK_RERANKER_MODEL` / `_TOKENIZER` | Reranker | P4 |
| `MAIEUTIK_LLM_MODEL` | Generator | P5 |
| `MAIEUTIK_OPENAI_API_KEY` | OpenAI provider (keyring preferred) | phase 4 |
| `MAIEUTIK_LAYA_URL` | Judge sidecar | P7 |

## Decisions taken by the owner

1. Lexical search: **SQLite FTS5 with BM25** (rusqlite, `bundled` feature),
   merged with dense results by RRF (P4).
2. Voice: **path B — sherpa-onnx in Rust** with Parakeet TDT 0.6B v3;
   OpenWhispr serves as an open-source reference for configuration only (P8).

## Open decisions (listed, not decided)

1. LLM model beyond the Qwen2.5 7B Q4 reference candidate (P5).
2. Reranker artifact format (which ONNX export) — decided when P4 starts.
