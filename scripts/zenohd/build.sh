#!/bin/bash
# Build zenohd from the local zenoh submodule
#
# This script builds zenohd from the pinned submodule at
# third-party/zenoh/zenoh/ to ensure version compatibility with
# rmw_zenoh_cpp.
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
ZENOH_DIR="$REPO_ROOT/third-party/zenoh/zenoh"

# Parse arguments
if [ "$1" = "--clean" ]; then
    echo "Cleaning zenohd build..."
    rm -rf "$BUILD_DIR"
    rm -rf "$ZENOH_DIR/target"
    echo "Done."
    exit 0
fi

# Already provisioned (e.g. by `just zenohd setup` / `nros setup --tool
# zenohd`)? Honour the existing binary so `build-zenohd` is idempotent and a
# prior prebuilt-fetch step satisfies it — the host CI lanes provision zenohd
# up front, so the `build-zenohd` recipe prereq must not re-fail here.
if [ -x "$BUILD_DIR/zenohd" ]; then
    echo "zenohd present: $BUILD_DIR/zenohd"
    exit 0
fi

# Check prerequisites
if ! command -v cargo &>/dev/null; then
    echo "Error: cargo not found"
    echo "Install Rust: https://rustup.rs"
    exit 1
fi

# Prefer the prebuilt zenohd from the nros SDK store (provisioned by
# `nros setup … --rmw zenoh`) — avoids rebuilding the large zenoh tree from
# source and the source submodule entirely. Publish it at build/zenohd/zenohd
# where tests + recipes look.
NROS_STORE="${NROS_HOME:-$HOME/.nros}/sdk"
store_zenohd="$(ls -d "$NROS_STORE"/zenohd/*/bin/zenohd 2>/dev/null | tail -1 || true)"
if [ -n "$store_zenohd" ] && [ -x "$store_zenohd" ]; then
    echo "Using prebuilt zenohd from the nros store: $store_zenohd"
    mkdir -p "$BUILD_DIR"
    tmp="$BUILD_DIR/zenohd.$$"
    install -m 0755 "$store_zenohd" "$tmp"
    mv -f "$tmp" "$BUILD_DIR/zenohd"
    "$BUILD_DIR/zenohd" --version
    exit 0
fi

# No store zenohd — fall back to a source build, but only if the submodule is
# already checked out. Provisioning is `nros setup`'s job; don't silently init
# submodules here.
if [ ! -f "$ZENOH_DIR/Cargo.toml" ]; then
    echo "Error: zenohd not provisioned." >&2
    echo "  Run:  nros setup native --rmw zenoh   (installs the prebuilt zenohd)" >&2
    echo "  Or, to build from source, first check out the submodule:" >&2
    echo "        git submodule update --init third-party/zenoh/zenoh" >&2
    exit 1
fi

echo "Building zenohd from submodule..."
echo "  Source: $ZENOH_DIR"
echo "  Output: $BUILD_DIR/zenohd"
echo ""

# Build zenohd with transport_serial feature
cd "$ZENOH_DIR"
cargo build -p zenohd --release --features "zenoh/transport_serial"

# Publish the binary via rename so rebuilds do not fail with ETXTBSY when
# an older build/zenohd/zenohd is still mapped by a running test router.
mkdir -p "$BUILD_DIR"
tmp="$BUILD_DIR/zenohd.$$"
install -m 0755 "$ZENOH_DIR/target/release/zenohd" "$tmp"
mv -f "$tmp" "$BUILD_DIR/zenohd"

# Show result
echo ""
echo "Build complete!"
echo "  Binary: $BUILD_DIR/zenohd"
"$BUILD_DIR/zenohd" --version
