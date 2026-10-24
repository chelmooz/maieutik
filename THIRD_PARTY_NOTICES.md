# Third-party notices

## Models (license to re-verify on the official model card before redistribution)

| Model | Used for | License per model card (to confirm) |
|---|---|---|
| BAAI/bge-m3 (ONNX fp32 export) | dense embeddings | MIT (per HF model card) |
| silero-vad (silero_vad.onnx) | voice activity detection | MIT (per upstream repo) |
| parakeet-tdt-0.6b-v3 tokens (tokens.txt) | STT vocabulary (unused until the STT phase) | not verified — do not redistribute until checked |
| Qwen GGUF (user-provided, runtime download) | local LLM | depends on the exact file (Apache-2.0 or Tongyi Qianwen LICENSE) — verify before use |

## Rust dependencies (direct, resolved versions from Cargo.lock)

| Crate | Section | Resolved version(s) | License (declared by the crate) |
|---|---|---|---|
| serde | dependencies | 1.0.229 [MIT OR Apache-2.0] | MIT OR Apache-2.0 |
| tokio | dependencies | 0.1.22 [MIT]; 1.54.1 [MIT] | MIT |
| config | dependencies | 0.14.1 [MIT OR Apache-2.0] | MIT OR Apache-2.0 |
| notify | dependencies | 6.1.1 [CC0-1.0] | CC0-1.0 |
| ort | dependencies | 2.0.0-rc.15 [MIT OR Apache-2.0] | MIT OR Apache-2.0 |
| llama-cpp-2 | dependencies | 0.1.160 [MIT OR Apache-2.0] | MIT OR Apache-2.0 |
| sherpa-onnx | dependencies | 1.13.8 [Apache-2.0] | Apache-2.0 |
| reqwest | dependencies | 0.12.28 [MIT OR Apache-2.0] | MIT OR Apache-2.0 |
| jsonrpsee | dependencies | 0.26.1 [MIT] | MIT |
| comrak | dependencies | 0.20.0 [BSD-2-Clause] | BSD-2-Clause |
| tracing-subscriber | dependencies | 0.3.23 [MIT] | MIT |
| clap | dependencies | 4.6.7 [MIT OR Apache-2.0] | MIT OR Apache-2.0 |
| cpal | dependencies | 0.15.3 [Apache-2.0] | Apache-2.0 |
| uuid | dependencies | 1.29.0 [Apache-2.0 OR MIT] | Apache-2.0 OR MIT |
| chrono | dependencies | 0.4.45 [MIT OR Apache-2.0] | MIT OR Apache-2.0 |
| pnet | dependencies | 0.35.0 [MIT OR Apache-2.0] | MIT OR Apache-2.0 |
| tokenizers | dependencies | 0.23.2 [Apache-2.0] | Apache-2.0 |

Transitive dependencies carry their own licences (see Cargo.lock).
For complete binary-distribution notices, regenerate with `cargo-about`.

Note: some crates resolve to several versions today (e.g. tokio 0.1 via
`tokio-named-pipes`); the duplicates are scheduled for removal (hygiene phase).
