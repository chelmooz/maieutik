#!/bin/bash
# Autoinstall script for Maieutik
# Usage: ./scripts/autoinstall.sh [--model bge-m3] [--device cpu|gpu] [--skip-build]

set -e

MODEL="${MODEL:-bge-m3}"
DEVICE="${DEVICE:-cpu}"
SKIP_BUILD="${SKIP_BUILD:-false}"

echo "🚀 Installation Maieutik"
echo "Mode: $MODEL | Device: $DEVICE"

# Check Rust
if ! command -v cargo &>/dev/null; then
    echo "❌ Rust non installé. Installez rustup:"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
    source "$HOME/.cargo/env"
fi

# Install cmake if needed
if ! command -v cmake &>/dev/null; then
    echo "🔧 Installation de cmake..."
    sudo apt-get update && sudo apt-get install -y cmake pkg-config
fi

# Build (skip if requested)
if [ "$SKIP_BUILD" != "true" ]; then
    echo "🔨 Compilation..."
    cargo build --release --locked --features cpp-modules
fi

# Download model
MODEL_DIR="$HOME/.local/share/maieutik/models/$MODEL"
mkdir -p "$MODEL_DIR"

# TODO: Add wget/curl download logic for model files
echo "📦 Téléchargement du modèle dans $MODEL_DIR"
# Placeholder: implement download logic

echo "✅ Installation terminée!"
echo "Utilisez: ./target/release/maieutik index"