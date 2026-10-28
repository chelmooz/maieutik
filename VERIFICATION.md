# Verification status — Maieutik

## Revision

- Multi-commit history on `main` (see `git log`).
- Reference revision: **R0** — the pre-publication state this tree was derived
  from. The top-1 retrieval distances below were measured at R0; they have
  not been replayed on this tree (see below).
- Toolchain: `rustc 1.99.1`, `cargo 1.99.1`.

## Source file binding (sha256, first 12 hex chars)

```
de390db8d538  Cargo.lock
d3e7931631cc  Cargo.toml
d45e04e9e055  build.rs
463ea4e147c5  src/config/mod.rs
229a0e521e41  src/config/store.rs
524da9647032  src/core/indexer/mod.rs
af2b9a17c0f5  src/core/mod.rs
5efa656c2534  src/core/ort_env.rs
898272ee6f98  src/core/scan.rs
5a23194fcd29  src/core/watch.rs
b8008c24b2be  src/lib.rs
8e4ce86d05a8  src/llm/mod.rs
8965ab6ccd12  src/main.rs
19ae552f9162  src/mcp/mod.rs
1c9c54b0a9cd  src/speech/mod.rs
91050b9b4da4  src/ui/mod.rs
c2dc2949d18c  src/utils/mod.rs
36d44a041b21  tests/config_store.rs
61734ffd9005  tests/chunker.rs
ae7766f9018c  tests/docx_extract.rs
82cdbbb9e228  tests/embedding_search.rs
32aad17f2324  tests/mcp_health.rs
4d1e2c21e798  tests/index_replace.rs
efe23d72c897  tests/index_scan.rs
09daa53bbdd7  tests/index_search.rs
b03abc7e7a34  tests/cli_exit_codes.rs
9668d7632b67  tests/single_instance.rs
b12b6d7bd70a  tests/watcher_reindex.rs
ecb78c064436  tests/watcher_shutdown.rs
f37b91078f63  tests/watch_service.rs
f02770620bc0  tests/startup_reconcile.rs
8b7a90fe43f1  tests/file_fingerprint.rs
bf36b221d71a  tests/force_recovery.rs
b93c5b1332eb  tests/pdf_extraction.rs
acf5418511a6  docs/INSTALLATION.md
4ec7db00797c  docs/UTILISATION.md
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
| 4 | `cargo test --locked --test embedding_search -- --ignored --nocapture --test-threads=1` (with model, replayed at P1.24) | exit 0 — 3 tests passed (15.08 s); top-1 distances identical to the archived log (see honesty notes) |
| 5 | same command after tightening `MAX_MATCH_DISTANCE` to 0.95 (P1.25) | exit 0 — 3 tests passed (14.43 s); the same command at 0.80 fails by design on query 1 (`d=0.8670, above the 0.8 threshold`) |
| 6 | `cargo audit --deny warnings` after bumping `single-instance` to 0.3.3 (P1.26) | exit 0 — 470 crates scanned, 5 documented exceptions; the two `failure` advisories (RUSTSEC-2019-0036, RUSTSEC-2020-0036) are gone because `failure` left the lock |
| 7 | `cargo clippy --locked --all-targets` after the backlog cleanup (P1.27) | exit 0 — `grep -c "^warning"` falls from 29 to 1, and the surviving line is the build script's informational "Skipping model download" (not a lint); the full test suite passes with 96 tests |
| 8 | `cargo test --locked --test mcp_health` (P1.28) | exit 0 — 2 tests passed: a real `run_unix_socket` server on a sandbox socket answers `health.check` with `ok` + the binary version over the framed protocol, and an unrouted method is refused by name. Before the fix both failed on `Some("Not implemented")` |

- Hashes re-bound at P1.28 (`src/mcp/mod.rs` changed, `tests/mcp_health.rs`
  added — `health.check` is the first routed MCP method); at P1.27
  (15 files — clippy backlog cleared: `is_empty`
  added to `HnswIndex`, `VectorIndex::open` loses its unused `settings`
  parameter and `db` field, unused imports/impls removed; red commit first for
  `is_empty`); at P1.26 (`Cargo.toml`, `Cargo.lock`, `src/main.rs`,
  `tests/single_instance.rs`, `tests/cli_exit_codes.rs`, `tests/watch_service.rs`
  — `single-instance` 0.1.2 → 0.3.3 changed the lock from a cwd-relative flock
  file to a process-global abstract socket, so the lock name is now
  overridable by `MAIEUTIK_INSTANCE_NAME` and the tests isolate their
  sandboxes); at P1.25 (`tests/embedding_search.rs`); at P1.23
  (`src/core/indexer/mod.rs`, `src/core/scan.rs`, `src/main.rs`,
  `docs/UTILISATION.md`, `tests/watch_service.rs`,
  `tests/startup_reconcile.rs` changed; `tests/file_fingerprint.rs` added —
  file fingerprint and drift detection at startup). Logs below predate P1.5
  and were not replayed.

Raw logs accompany this document in the review archive. They predate P1.5
and were **not** replayed; only the source hashes above are re-bound at each
lot. Exception: the embedder tests were replayed at P1.24 on this tree (row 4
above, same CPU as the archived run), with identical output.

### Retrieval results and thresholds — honesty notes

Top-1 Euclidean distances from `test-model-nocapture.log` (R0):
**0.8670 / 0.7834 / 0.7049** — replayed at P1.24 on this tree:
**identical** (same model files, same corpus, deterministic embeddings).

- An earlier revision of this file claimed `MAX_MATCH_DISTANCE = 1.05` had
  been "calibrated on an earlier, smaller model" — that claim was wrong: the
  test's own doc comment documents measurements taken **with BGE-M3**
  (0.87 / 0.78 / 0.70), and the P1.24 replay reproduces them exactly. No
  recalibration was needed.
- The threshold was **tightened from 1.05 to 0.95 at P1.25**: right answers
  measured 0.8670 / 0.7834 / 0.7049, best **wrong-file** distances
  1.0923 / 1.1823 / 1.0948, so 0.95 keeps ≥ 0.08 of margin above the worst
  right answer and ≥ 0.14 below the best wrong one. A value below 0.87 fails
  by design (verified at 0.80: `d=0.8670, above the 0.8 threshold`).
  `MIN_FILE_MARGIN = 0.15` (measured margins at P1.24:
  0.2253 / 0.3989 / 0.3899) remains the binding assertion.
- `MIN_FILE_MARGIN` is based on **3 English queries over 3 documents**.
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
   (`tests/single_instance.rs`). At P1.26 the crate was bumped to 0.3.3
   (no more `failure`); its Linux lock is a **process-global** abstract socket
   instead of a cwd-relative flock file — the same machine can no longer run
   two instances from different directories, which matches the shared index
   database. `MAIEUTIK_INSTANCE_NAME` exists so tests can isolate sandboxes;
   it is not a user-facing feature. ~~MCP socket path uses the caller PID under
   `/tmp`~~ **Fixed in P1.17**: `prepare_socket_path` only removes a stale
   socket (never a regular file) and `socket_path()` lives under the user's
   private runtime directory (`tests` in `src/mcp/mod.rs`). The server
   routed nothing until P1.28: `dispatch` answered "Not implemented" to
   every method. `health.check` is now the only routed one
   (`tests/mcp_health.rs`); no binary command launches the server yet
   (`--mcp-stdio` exits 3).
9. Secrets: keyring and open_ai_* settings removed at P1.12; config.json is written with mode 0600 on Unix.
10. ~~`nom v2.2.1` (transitive) triggers a future-incompat warning~~
   **Obsolete on this tree**: `nom` 2.2.1 no longer appears in `Cargo.lock`
   (only 7.1.3 and 8.0.0 remain), and the P1.27 clippy run emits no
   future-incompat warning.
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
