# Maieutik

Local-first notes with AI, in Rust. **Experimental quality bar**: the core
(semantic indexing and search over a real embedding model) is tested; the rest
is explicit stubs (see *Limits*).

## What works

- Semantic indexing of text: BGE-M3 dense embeddings (1024-dim, official ONNX
  export), XLM-RoBERTa tokenizer, sled storage + HNSW (fast-hnsw).
- A corpus → chunk → embed → search integration test with measured thresholds
  (`tests/embedding_search.rs`).
- CLI (`cargo run -- --help`), JSON settings, file
  watcher skeleton, MCP bridge skeleton.

## Requirements (Arch Linux)

- Rust (tested with 1.99.1; the true minimum version has not been measured yet),
`cmake`, `clang`, `pkg-config`
- `onnxruntime` (tested with 1.31.0), exposed via `ORT_DYLIB_PATH`, e.g.
  `export ORT_DYLIB_PATH=/usr/lib/libonnxruntime.so`
- For the model-dependent tests only:
  - `MAIEUTIK_EMBEDDER_MODEL` → `model.onnx` (BAAI/bge-m3 ONNX export, fp32 —
    note `model.onnx_data` (2.27 GB) must sit next to it)
  - `MAIEUTIK_EMBEDDER_TOKENIZER` → matching `tokenizer.json`
- Optional: `MAIEUTIK_DOWNLOAD_MODELS=1` makes
  `build.rs` fetch small models at build time (off by default; the app is
  meant to download models at runtime into the user data directory).

## Build & test

```sh
cargo check --locked --all-targets          # 0 errors expected
cargo test --locked                          # model-independent tests
# with the real model:
MAIEUTIK_EMBEDDER_MODEL=… MAIEUTIK_EMBEDDER_TOKENIZER=… ORT_DYLIB_PATH=… \
  cargo test --locked -- --ignored           # 3 integration tests
```

## Environment variables

| Variable | Meaning |
|---|---|
| `MAIEUTIK_EMBEDDER_MODEL` | path to the ONNX model (default: `models_dir()/bge-m3/model.onnx`) |
| `MAIEUTIK_EMBEDDER_TOKENIZER` | path to `tokenizer.json` (default alongside the model) |
| `MAIEUTIK_EMBEDDER_THREADS` | intra-op threads (default: machine parallelism) |
| `ORT_DYLIB_PATH` | path to `libonnxruntime.so` (required with `load-dynamic`) |

## Limits (known, by design for now)

- The indexing pipeline is **not wired** end to end (watcher → extract →
  chunk → index); `maieutik search` over real files returns nothing yet.
- The MCP module is a JSON-RPC bridge skeleton, not a spec-compliant MCP
  server.
- The chunker **panics on non-ASCII text when overlap > 0**, and heading
  offsets are wrong on CRLF files. Fix scheduled (core phase).
- The llama.cpp generation loop is written against the real API but has never
  loaded a model in this environment.
- Sparse/ColBERT outputs of BGE-M3 are absent from the ONNX export used here
  (dense retrieval only).

See `VERIFICATION.md` for what was actually run. Raw logs are produced locally
(gitignored) and travel with the review archives — they are not part of this
repository.
