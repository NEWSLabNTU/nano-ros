#!/bin/bash
# Build Micro-XRCE-DDS Agent from source
#
# Builds Micro-XRCE-DDS-Agent from the submodule at third-party/xrce/agent.
# The Agent is needed for XRCE-DDS integration tests (just xrce test).
#
# Usage:
#   ./scripts/xrce-agent/build.sh [--clean]
#
# Output:
#   build/xrce-agent/MicroXRCEAgent
#
# Prerequisites:
#   - CMake >= 3.5
#   - C++14 compiler (gcc >= 5, clang >= 3.4)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
AGENT_SRC="$REPO_ROOT/third-party/xrce/agent"
BUILD_DIR="$REPO_ROOT/build/xrce-agent"

# Parse arguments
if [ "$1" = "--clean" ]; then
    echo "Cleaning XRCE Agent build..."
    rm -rf "$BUILD_DIR"
    echo "Done."
    exit 0
fi

# Prefer the prebuilt MicroXRCEAgent from the nros SDK store (provisioned by
# `nros setup … --rmw xrce`) — no source build, no submodule, no cmake/g++
# needed. Publish it at build/xrce-agent/MicroXRCEAgent where tests + recipes
# look. Source build below is the fallback for trees without nros provisioning.
NROS_STORE="${NROS_HOME:-$HOME/.nros}/sdk"
store_agent="$(ls -d "$NROS_STORE"/xrce-agent/*/bin/MicroXRCEAgent 2>/dev/null | tail -1 || true)"
if [ -n "$store_agent" ] && [ -x "$store_agent" ]; then
    echo "Using prebuilt Micro-XRCE-DDS Agent from the nros store: $store_agent"
    # The store binary is a relocatable launcher that resolves its bundled
    # `../lib/MicroXRCEAgent.real` relative to itself — so it must run from its
    # own dir. Publish a forwarding wrapper (not a copy) at the expected path.
    mkdir -p "$BUILD_DIR"
    tmp="$BUILD_DIR/MicroXRCEAgent.$$"
    printf '#!/bin/sh\nexec "%s" "$@"\n' "$store_agent" > "$tmp"
    chmod 0755 "$tmp"
    mv -f "$tmp" "$BUILD_DIR/MicroXRCEAgent"
    "$BUILD_DIR/MicroXRCEAgent" --version 2>/dev/null || true
    exit 0
fi

# Check prerequisites
if ! command -v cmake &>/dev/null; then
    echo "Error: cmake not found"
    echo "Install: sudo apt install cmake"
    exit 1
fi

if ! command -v g++ &>/dev/null && ! command -v clang++ &>/dev/null; then
    echo "Error: C++ compiler not found"
    echo "Install: sudo apt install g++"
    exit 1
fi

# Ensure the submodule is initialized (auto-init on a fresh/deinit'd tree).
if [ ! -f "$AGENT_SRC/CMakeLists.txt" ]; then
    echo "XRCE Agent submodule not checked out — initializing third-party/xrce/agent..."
    git -C "$REPO_ROOT" submodule update --init --recursive third-party/xrce/agent
fi
if [ ! -f "$AGENT_SRC/CMakeLists.txt" ]; then
    echo "Error: XRCE Agent submodule still missing at $AGENT_SRC after init" >&2
    exit 1
fi

echo "Building Micro-XRCE-DDS Agent..."
echo "  Source: $AGENT_SRC"
echo "  Output: $BUILD_DIR/MicroXRCEAgent"
echo ""

# Configure and build
mkdir -p "$BUILD_DIR"
cd "$BUILD_DIR"
cmake "$AGENT_SRC" \
    -DUAGENT_BUILD_EXECUTABLE=ON \
    -DUAGENT_P2P_PROFILE=OFF \
    -DUAGENT_LOGGER_PROFILE=OFF \
    -DCMAKE_BUILD_TYPE=Release

cmake --build . --parallel "$(nproc 2>/dev/null || echo 4)"

# Verify
if [ ! -f "$BUILD_DIR/MicroXRCEAgent" ]; then
    echo "Error: MicroXRCEAgent binary not found after build"
    exit 1
fi

echo ""
echo "Build complete!"
echo "  Binary: $BUILD_DIR/MicroXRCEAgent"
