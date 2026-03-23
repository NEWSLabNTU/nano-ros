#!/bin/bash
# Build zenohd from the local zenoh submodule
#
# This script builds zenohd 1.6.2 from the pinned submodule at
# scripts/zenohd/zenoh/ to ensure version compatibility with
# rmw_zenoh_cpp (ros-humble-zenoh-cpp-vendor 0.1.8).
#
# Usage:
#   ./scripts/zenohd/build.sh [--clean]
#
# Output:
#   build/zenohd/zenohd
#
# Prerequisites:
#   - Rust toolchain (1.85.0+ for zenoh 1.6.2)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="$REPO_ROOT/build/zenohd"
ZENOH_DIR="$SCRIPT_DIR/zenoh"

# Parse arguments
if [ "$1" = "--clean" ]; then
    echo "Cleaning zenohd build..."
    rm -rf "$BUILD_DIR"
    rm -rf "$ZENOH_DIR/target"
    echo "Done."
    exit 0
fi

# Check prerequisites
if ! command -v cargo &>/dev/null; then
    echo "Error: cargo not found"
    echo "Install Rust: https://rustup.rs"
    exit 1
fi

# Check zenoh submodule
if [ ! -f "$ZENOH_DIR/Cargo.toml" ]; then
    echo "Error: zenoh submodule not found at $ZENOH_DIR"
    echo "Run: git submodule update --init scripts/zenohd/zenoh"
    exit 1
fi

echo "Building zenohd from submodule..."
echo "  Source: $ZENOH_DIR"
echo "  Output: $BUILD_DIR/zenohd"
echo ""

# Build zenohd with transport_serial feature
cd "$ZENOH_DIR"
cargo build -p zenohd --release --features "zenoh/transport_serial"

# Copy binary to build directory
mkdir -p "$BUILD_DIR"
cp "$ZENOH_DIR/target/release/zenohd" "$BUILD_DIR/zenohd"

# Show result
echo ""
echo "Build complete!"
echo "  Binary: $BUILD_DIR/zenohd"
"$BUILD_DIR/zenohd" --version
