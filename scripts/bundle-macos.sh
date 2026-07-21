#!/bin/bash
# Build, ad-hoc sign, and (optionally) install murmr as a macOS .app bundle.
#
# Usage:
#   ./scripts/bundle-macos.sh            # build + sign
#   ./scripts/bundle-macos.sh --install  # also install to /Applications and launch
#
# Note on permissions: this is an ad-hoc-signed local build. macOS ties TCC
# grants (Input Monitoring, Accessibility, Microphone) to the code signature,
# which changes on each build, so a rebuild can drop those grants. If the hotkey
# stops working after a rebuild, reset and re-grant:
#   tccutil reset Accessibility com.arvmaan.murmer
#   tccutil reset ListenEvent   com.arvmaan.murmer
#   tccutil reset Microphone    com.arvmaan.murmer
# Proper persistence needs a Developer ID certificate (paid Apple account).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(dirname "$SCRIPT_DIR")"
APP="$PROJECT_DIR/target/release/bundle/macos/murmer.app"

echo "Building murmr.app…"
( cd "$PROJECT_DIR" && cargo tauri build --features bedrock )

echo "Ad-hoc signing…"
codesign --force --deep --sign - "$APP"

echo ""
echo "Built: $APP"
echo "DMG:   $PROJECT_DIR/target/release/bundle/dmg/"

if [[ "${1:-}" == "--install" ]]; then
    echo "Installing to /Applications…"
    pkill -f "murmer-app" 2>/dev/null || true
    rm -rf /Applications/murmer.app
    cp -R "$APP" /Applications/
    open /Applications/murmer.app
    echo "Installed and launched."
    echo "If the hotkey or paste stops working after a rebuild, re-grant Input"
    echo "Monitoring + Accessibility (macOS may drop them when the build changes)."
fi
