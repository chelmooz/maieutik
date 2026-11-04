#!/bin/bash
export RUST_LOG=info
exec cargo run --bin maieutik-mcp --manifest-path /home/chelmooz/Work/maieutik-corrige/laya-primitives/Cargo.toml "$@"
