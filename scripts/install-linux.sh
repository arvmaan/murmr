#!/bin/bash
# Install murmer on Linux
# Usage: ./scripts/install-linux.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"

echo "Building murmer (release)..."
cargo build --release

BINARY="$PROJECT_DIR/target/release/murmer"
INSTALL_DIR="${HOME}/.local/bin"

mkdir -p "$INSTALL_DIR"
cp "$BINARY" "$INSTALL_DIR/murmer"

echo ""
echo "Installed to: $INSTALL_DIR/murmer"
echo ""
echo "Prerequisites:"
echo "  - Ollama running: ollama serve"
echo "  - Models pulled: ollama pull qwen3:1.7b && ollama pull phi4-mini"
echo "  - Whisper model: murmer --download-model base.en"
echo "  - Wayland: sudo apt install wtype wl-clipboard"
echo "  - X11: sudo apt install xdotool xclip"
echo ""
echo "Run: murmer --check"
