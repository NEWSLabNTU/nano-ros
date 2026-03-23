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

# Verify submodule is initialized
if [ ! -f "$AGENT_SRC/CMakeLists.txt" ]; then
    echo "Error: XRCE Agent submodule not initialized at $AGENT_SRC"
    echo "Run: git submodule update --init third-party/xrce/agent"
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
