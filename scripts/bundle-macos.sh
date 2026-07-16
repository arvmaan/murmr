#!/bin/bash
# Build murmer as a macOS .app bundle
# Usage: ./scripts/bundle-macos.sh [--release]

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
APP_NAME="murmer"
BUNDLE_DIR="$PROJECT_DIR/target/bundle"
APP_DIR="$BUNDLE_DIR/$APP_NAME.app"

# Determine build profile
if [[ "${1:-}" == "--release" ]]; then
    PROFILE="release"
    cargo build --release
    BINARY="$PROJECT_DIR/target/release/$APP_NAME"
else
    PROFILE="debug"
    cargo build
    BINARY="$PROJECT_DIR/target/debug/$APP_NAME"
fi

echo "Building $APP_NAME.app ($PROFILE)..."

# Create .app bundle structure
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"

# Copy binary
cp "$BINARY" "$APP_DIR/Contents/MacOS/$APP_NAME"

# Copy Info.plist
cp "$PROJECT_DIR/macos/Info.plist" "$APP_DIR/Contents/"

# Create a minimal icon if none exists
if [[ -f "$PROJECT_DIR/icons/icon.icns" ]]; then
    cp "$PROJECT_DIR/icons/icon.icns" "$APP_DIR/Contents/Resources/icon.icns"
fi

echo ""
echo "Created: $APP_DIR"
echo ""
echo "To install:"
echo "  cp -r $APP_DIR /Applications/"
echo ""
echo "On first run, macOS will prompt for:"
echo "  - Microphone access"
echo "  - Accessibility (for global hotkeys)"
echo "  - Input Monitoring (for keyboard simulation)"
echo ""
echo "Grant all three in System Settings → Privacy & Security"
