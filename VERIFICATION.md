# Verification status — Maieutik

## Revision

- Multi-commit history on `main` (see `git log`).
- Reference revision: **R0** — the pre-publication state this tree was derived
  from. The top-1 retrieval distances below were measured at R0; they have
  not been replayed on this tree (see below).
- Toolchain: `rustc 1.99.1`, `cargo 1.99.1`.

## Source file binding (sha256, first 12 hex chars)

```
70d2058787c6  Cargo.lock
9135f2b76f0b  Cargo.toml
d45e04e9e055  build.rs
a622ac8a2f56  src/config/mod.rs
229a0e521e41  src/config/store.rs
9e228225c891  src/core/indexer/mod.rs
af2b9a17c0f5  src/core/mod.rs
5efa656c2534  src/core/ort_env.rs
2c5dacae9b70  src/core/scan.rs
3d85c972fb77  src/core/watch.rs
41fd305a17b7  src/lib.rs
43e08d9772de  src/llm/mod.rs
7a7ccb798f8a  src/main.rs
02b7820e7098  src/mcp/mod.rs
a8d9e106e24a  src/speech/mod.rs
91050b9b4da4  src/ui/mod.rs
1756d1fa548c  src/utils/mod.rs
36d44a041b21  tests/config_store.rs
61734ffd9005  tests/chunker.rs
ae7766f9018c  tests/docx_extract.rs
28139d62c849  tests/embedding_search.rs
4d1e2c21e798  tests/index_replace.rs
077770b9bbb3  tests/index_scan.rs
9e6548501da4  tests/index_search.rs
997a0969cb10  tests/cli_exit_codes.rs
4ae38f5b1297  tests/single_instance.rs
5eba94868486  tests/watcher_reindex.rs
ecb78c064436  tests/watcher_shutdown.rs
25318c4749a6  tests/watch_service.rs
6a27a969f5fd  tests/startup_reconcile.rs
fe4639e27aa1  tests/force_recovery.rs
b93c5b1332eb  tests/pdf_extraction.rs
251fb2ddad73  docs/INSTALLATION.md
d9ca47d08d48  docs/UTILISATION.md
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

- Hashes re-bound at P1.22 (`src/core/scan.rs`, `src/main.rs`,
  `docs/UTILISATION.md`, `tests/watch_service.rs` changed;
  `tests/startup_reconcile.rs` added — startup reconciliation); logs below
  predate P1.5 and were not replayed.

Raw logs accompany this document in the review archive. They predate P1.5
and were **not** replayed on this tree; only the source hashes above are
re-bound at each lot.

### Retrieval results and thresholds — honesty notes

Top-1 Euclidean distances from `test-model-nocapture.log`:
**0.8670 / 0.7834 / 0.7049** — measured at reference revision R0 and not
replayed since the P1.5 cleanup (names, comments and an unused module changed).

- `MAX_MATCH_DISTANCE = 1.05` was calibrated on an earlier, smaller model and
  is **not recalibrated for BGE-M3** (≈ cosine 0.45 — filters almost nothing).
- `MIN_FILE_MARGIN = 0.15` is the binding assertion, based on **3 English
  queries over 3 documents**.
- The test corpus is **English only**; French/cross-lingual retrieval is
  **not tested yet** (planned: evaluation phase).

## Known issues (open)

1. ~~**Watcher does not re-index**~~ **Fixed in P1.16**: created, modified and
   rename-target files are re-indexed via `core::scan::index_path`
   (validated: extension, size fail-closed), rename sources are purged and
   removal errors are logged (`tests/watcher_reindex.rs`). Events can still
   be lost if the process is down when they occur — no startup reconciliation
   yet: run `maieutik index` after downtime.
2. ~~Chunker panic on non-ASCII text and wrong heading offsets on CRLF files~~
   **Fixed in P1.8**: chunk size and overlap are counted in characters, byte
   offsets are always valid, and heading offsets are exact on CRLF files
   (`tests/chunker.rs`, no model needed).
3. **HNSW fully rebuilt per `upsert_file`**; stale chunks are purged
   atomically since P1.9 (`replace_file_records`, `tests/index_replace.rs`).
4. **Meta mismatch purges without re-indexing**; no migration test yet.
5. **Silero VAD v5 input shapes unverified** (`h`/`c` [1,128] vs `state`
   [2,1,128]). `Embedder` + `SileroVad` share the ORT environment; no test
   builds both together.
6. **llama.cpp sampler chain likely incomplete**; 512-token batch cap; no
   `n_ctx` check; blocking the tokio runtime; `special=true` tokenization of
   user content (prompt-injection surface).
7. `embed_batch` is sequential.
8. ~~`single-instance` never checks `is_single()`~~ **Fixed in P1.16**: the
   lock is enforced via `is_single()` and a second instance exits with code 4
   (`tests/single_instance.rs`). ~~MCP socket path uses the caller PID under
   `/tmp`~~ **Fixed in P1.17**: `prepare_socket_path` only removes a stale
   socket (never a regular file) and `socket_path()` lives under the user's
   private runtime directory (`tests` in `src/mcp/mod.rs`).
9. Secrets: keyring and open_ai_* settings removed at P1.12; config.json is written with mode 0600 on Unix.
10. `nom v2.2.1` (transitive) triggers a future-incompat warning.
11. `extract_headings` keeps the extra `#` of levels 2-6 in `Heading.text`
    (`## B` gives `# B`) and counts `#tag` as a heading; unchanged in P1.8.

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
