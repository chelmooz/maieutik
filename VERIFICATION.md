# Verification status — Maieutik

## Revision

- Single-commit history (fresh `main`, publication-ready tree).
- Reference revision: **R0** — the pre-publication state this tree was derived
  from. Behavior is unchanged from R0: top-1 retrieval distances are
  bit-identical (see below).
- Toolchain: `rustc 1.99.1`, `cargo 1.99.1`.

## Source file binding (sha256, first 12 hex chars)

```
27db23b9c48d  Cargo.lock
2303f229a368  Cargo.toml
d45e04e9e055  build.rs
a2a26107a9b6  src/config/mod.rs
eccda2009436  src/config/store.rs
9ce5529fab7a  src/core/indexer/mod.rs
6e4b4102caff  src/core/mod.rs
5efa656c2534  src/core/ort_env.rs
8ad1c4ffa09f  src/core/watch.rs
7b22658d7ed8  src/drive/mod.rs
fd631300701d  src/lib.rs
d13b49258d75  src/llm/mod.rs
7e0e64e34c18  src/main.rs
9d039a3c4182  src/mcp/mod.rs
a8d9e106e24a  src/speech/mod.rs
91050b9b4da4  src/ui/mod.rs
552b75b369df  src/utils/mod.rs
28139d62c849  tests/embedding_search.rs
```

## Embedder: BAAI/bge-m3

- Official ONNX export, fp32: `model.onnx` + `model.onnx_data` (2.27 GB,
  external data beside the model) + `tokenizer.json` (XLM-RoBERTa Unigram,
  250k vocabulary). SHA-256 of the development copies:
  - `model.onnx` → `f84251230831afb359ab26d9fd37d5936d4d9bb5d1d5410e66442f630f24435b`
  - `model.onnx_data` → `1eebfb28493f67bba03ce0ef64bfdc7fc5a3bd9d7493f818bb1d78cd798416b4`
  - `tokenizer.json` → `6710678b12670bc442b99edc952c4d996ae309a7020c1fa0096dd245c2faf790`
- Dense output only (`sentence_embedding`, [batch, 1024]). The pooling the
  graph performs internally is not independently verified; the code
  L2-normalizes on top. Sparse/ColBERT outputs are absent from this export.
- No query prefix. Truncation 8192 enforced in code. Intra-op threads via
  `MAIEUTIK_EMBEDDER_THREADS` (default = machine parallelism).

## Runtime environment (as used below)

- **libonnxruntime 1.31.0** (official GitHub release), loaded dynamically via
  `ORT_DYLIB_PATH` (`ort` 2.0.0-rc.15, `load-dynamic`, `api-31`).
- CPU: Xeon E5-2698 v3 (Haswell, no AVX-512 → int8 export variants unusable).

## What was actually run (this tree)

| # | Command | Result |
|---|---|---|
| 1 | `cargo check --locked --all-targets` | **0 errors**, warnings only (`check.log`) |
| 2 | `cargo test --locked` (no model, default parallelism) | exit 0 — 1 unit test, 3 model tests ignored by design (`test-no-model.log`) |
| 3 | `cargo test --locked -- --ignored --nocapture --test-threads=1` (with model) | exit 0 — 3 integration tests passed (14.1 s) (`test-model-nocapture.log`) |

Raw logs accompany this document in the review archive. They were produced on
this exact tree (after the last source edit) and were **not** replayed for
packaging.

### Retrieval results and thresholds — honesty notes

Top-1 Euclidean distances from `test-model-nocapture.log`:
**0.8670 / 0.7834 / 0.7049** — identical to reference revision R0 (the
cleanup changed names, comments and removed an unused module; behavior is
unchanged).

- `MAX_MATCH_DISTANCE = 1.05` was calibrated on an earlier, smaller model and
  is **not recalibrated for BGE-M3** (≈ cosine 0.45 — filters almost nothing).
- `MIN_FILE_MARGIN = 0.15` is the binding assertion, based on **3 English
  queries over 3 documents**.
- The test corpus is **English only**; French/cross-lingual retrieval is
  **not tested yet** (planned: evaluation phase).

## Known issues (open)

1. **Indexing pipeline not wired**: the watcher has a re-index TODO,
   the `index` CLI command is an empty stub, nothing calls `upsert_file`.
2. **Chunker panics on non-ASCII text when overlap > 0** (byte slicing at
   chunk boundaries — reproduced: `Chunker::new(10, 3)` on `"éééééééééééé"`
   panics with `byte index 7 is not a char boundary; it is inside 'é'`), and
   **heading offsets are wrong on CRLF files** (`lines()` drops `\r`; measured
   offset 16 vs real 18). Fix scheduled (core phase).
3. **HNSW fully rebuilt per `upsert_file`**; a file's stale chunks are not
   purged when it shrinks.
4. **Meta mismatch purges without re-indexing**; no migration test yet.
5. **Silero VAD v5 input shapes unverified** (`h`/`c` [1,128] vs `state`
   [2,1,128]). `Embedder` + `SileroVad` share the ORT environment; no test
   builds both together.
6. **llama.cpp sampler chain likely incomplete**; 512-token batch cap; no
   `n_ctx` check; blocking the tokio runtime; `special=true` tokenization of
   user content (prompt-injection surface).
7. `embed_batch` is sequential.
8. `single-instance` never checks `is_single()`; MCP socket path uses the
   caller PID under `/tmp`.
9. Secrets: keyring store not initialized (`NoDefaultStore` until a native
   store is registered); `open_ai_api_key` still stored in plain config JSON.
10. `nom v2.2.1` (transitive) triggers a future-incompat warning.

## Third-party attributions

Dependency and model licences are listed in `THIRD_PARTY_NOTICES.md`. Model
licences must be re-verified on their official model cards before any
redistribution.

## Reproducibility

- `build.rs` downloads nothing by default (`MAIEUTIK_DOWNLOAD_MODELS=1` to
  enable); never writes into the source tree.
- Default features `[]` (CPU). `cuda`, `vulkan`, `metal` opt-in, **not** built
  in the runs above.
- Logs `check.log`, `test-no-model.log`, `test-model-nocapture.log` travel
  with the review archive; they are not committed (gitignored).
