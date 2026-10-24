#!/usr/bin/env bash
# Release build that keeps personal paths out of the binary.
#
# rustc embeds absolute source paths in panic locations. On a developer
# machine these point at $HOME (registry crates) and at the project
# directory. This script computes --remap-path-prefix flags for $HOME,
# $CARGO_HOME and the project directory, then builds.
#
# Usage: scripts/release.sh [extra cargo args...]
# Proof of effect: strings target/release/maieutik | grep -c "/home/"  -> 0
set -euo pipefail
cd "$(dirname "$0")/.."

# Non-interactive shells (CI, hooks) may not have ~/.cargo/bin on PATH.
if ! command -v cargo >/dev/null 2>&1; then
  export PATH="${HOME:-}/.cargo/bin:$PATH"
fi

flags=()
add_remap() { # from to — only if `from` is a non-empty, existing path
  if [ -n "${1:-}" ] && [ -e "$1" ]; then
    flags+=("--remap-path-prefix=$1=$2")
  fi
}

# The cargo registry lives under CARGO_HOME (default: $HOME/.cargo) — remap
# it BEFORE $HOME so the more specific prefix wins.
if [ -n "${CARGO_HOME:-}" ]; then
  add_remap "$CARGO_HOME" "/cargo-registry"
else
  add_remap "${HOME:-}/.cargo" "/cargo-registry"
fi
add_remap "${HOME:-}" "/build-host"
add_remap "$PWD" "/workspace"

if [ ${#flags[@]} -gt 0 ]; then
  export RUSTFLAGS="${RUSTFLAGS:-} ${flags[*]}"
fi
echo "RUSTFLAGS: ${RUSTFLAGS:-<none>}"
exec cargo build --release "$@"
