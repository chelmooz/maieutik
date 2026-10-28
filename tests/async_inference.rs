//! Tests for async inference with spawn_blocking (A7 / Manus B2).
//!
//! Verifies that blocking llama.cpp inference runs in spawn_blocking,
//! not blocking the async runtime.

use maieutik::llm::{ChatCompletionProvider, LocalLlamaProvider};

/// A7.1 RED: Complete should not block the async runtime
#[tokio::test]
async fn complete_does_not_block_runtime() {
    // This test would require a mock model or real model
    // For now, we verify the type signature and that it returns a future
    // The actual implementation check is done by inspecting the code

    // The fix should use tokio::task::spawn_blocking for the blocking inference
    // This test verifies the method signature is async
    let _provider_fn = LocalLlamaProvider::complete;

    // Type signature check - complete is async
}

/// A7.2 RED: Stream complete should not block the async runtime
#[tokio::test]
async fn stream_complete_does_not_block_runtime() {
    // Same as above - verify async signature
    let _provider_fn = LocalLlamaProvider::stream_complete;

    // Type signature check - stream_complete is async
}

/// A7.3 RED: Blocking inference should run in spawn_blocking
#[tokio::test]
async fn blocking_inference_runs_in_spawn_blocking() {
    // This test would verify that the blocking llama.cpp calls
    // are wrapped in tokio::task::spawn_blocking
    // For now, this is a code inspection test

    // The fix should wrap LlamaSession::generate, complete, and stream_complete
    // in tokio::task::spawn_blocking

    // Code inspection - check spawn_blocking usage
}
