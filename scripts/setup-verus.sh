#!/usr/bin/env bash
set -euo pipefail
echo "=== Verus Setup ==="
VERUS_DIR="tools"
VERUS_BIN="$VERUS_DIR/verus"
if [ -x "$VERUS_BIN" ]; then
    echo "Verus already installed at $VERUS_BIN"
    "$VERUS_BIN" --version
    exit 0
fi
# Determine platform suffix for release asset
OS=$(uname -s)
ARCH=$(uname -m)
case "$OS-$ARCH" in
    Linux-x86_64)   PLATFORM="x86-linux" ;;
    Darwin-x86_64)  PLATFORM="x86-macos" ;;
    Darwin-arm64)   PLATFORM="arm64-macos" ;;
    Darwin-aarch64) PLATFORM="arm64-macos" ;;
    *)              echo "Unsupported platform: $OS-$ARCH"; exit 1 ;;
esac
# Query GitHub API for latest release download URL
API_URL="https://api.github.com/repos/verus-lang/verus/releases/latest"
echo "Querying latest Verus release..."
DOWNLOAD_URL=$(curl -fsSL "$API_URL" | python3 -c "import sys,json;[print(a['browser_download_url']) for a in json.load(sys.stdin)['assets'] if a['name'].endswith('-${PLATFORM}.zip')]" | head -1)
if [ -z "$DOWNLOAD_URL" ]; then
    echo "ERROR: No release asset found for platform $PLATFORM"
    exit 1
fi
echo "Downloading $DOWNLOAD_URL..."
ZIPFILE="/tmp/verus-${PLATFORM}.zip"
curl -fsSL "$DOWNLOAD_URL" -o "$ZIPFILE"
# Extract to tools/ (zip contains verus-<platform>/ directory)
TMPDIR=$(mktemp -d)
unzip -q "$ZIPFILE" -d "$TMPDIR"
mkdir -p "$VERUS_DIR"
cp -r "$TMPDIR"/verus-${PLATFORM}/* "$VERUS_DIR/"
rm -rf "$TMPDIR" "$ZIPFILE"
chmod +x "$VERUS_BIN" "$VERUS_DIR/cargo-verus" "$VERUS_DIR/z3" "$VERUS_DIR/rust_verify"
# Install required Rust toolchain
REQUIRED_TC=$("$VERUS_BIN" --version 2>&1 | grep 'Toolchain:' | sed 's/.*Toolchain: //' || true)
if [ -n "$REQUIRED_TC" ]; then
    echo "Installing required toolchain: $REQUIRED_TC"
    rustup toolchain install "$REQUIRED_TC"
fi
"$VERUS_BIN" --version
echo "Verus setup complete."
