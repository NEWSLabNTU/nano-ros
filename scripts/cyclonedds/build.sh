#!/bin/bash
# Build Cyclone DDS from the pinned submodule.
#
# Configures + builds + installs Cyclone DDS into the project's shared
# install prefix (`build/install/`), so downstream consumers (the
# `nros-rmw-cyclonedds` C++ backend in Phase 117 and the
# `cargo nano-ros generate-cyclonedds-types` codegen path) can locate
# `libddsc.a`, the public headers, and the `idlc` IDL compiler via
# `find_package(CycloneDDS)` against `CMAKE_PREFIX_PATH=build/install`.
#
# Pin: tag 0.10.5 (matches `ros-humble-cyclonedds` 0.10.5 +
# `ros-humble-rmw-cyclonedds-cpp` 1.3.4 → wire-compat for ROS 2 Humble).
#
# Usage:
#   ./scripts/cyclonedds/build.sh [--clean]
#
# Output:
#   build/cyclonedds/                 # cmake build tree
#   build/install/lib/libddsc.{a,so}  # static + shared library
#   build/install/include/dds/*.h     # public headers
#   build/install/bin/idlc            # IDL compiler (used by codegen)
#   build/install/lib/cmake/CycloneDDS/CycloneDDSConfig.cmake
#
# Prerequisites:
#   - cmake (>= 3.16)
#   - C compiler (gcc / clang)
#   - Cyclone submodule fetched: `git submodule update --init
#     third-party/dds/cyclonedds`

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="$REPO_ROOT/build/cyclonedds"
INSTALL_DIR="$REPO_ROOT/build/install"
CYCLONEDDS_DIR="${CYCLONEDDS_DIR:-$REPO_ROOT/third-party/dds/cyclonedds}"

if [ "$1" = "--clean" ]; then
    echo "Cleaning Cyclone DDS build..."
    rm -rf "$BUILD_DIR"
    echo "Done."
    exit 0
fi

if ! command -v cmake &>/dev/null; then
    echo "Error: cmake not found"
    exit 1
fi

if [ ! -f "$CYCLONEDDS_DIR/CMakeLists.txt" ]; then
    echo "Error: Cyclone DDS submodule not found at $CYCLONEDDS_DIR"
    echo "Run: git submodule update --init third-party/dds/cyclonedds"
    exit 1
fi

echo "Building Cyclone DDS from submodule..."
echo "  Source:  $CYCLONEDDS_DIR"
echo "  Build:   $BUILD_DIR"
echo "  Install: $INSTALL_DIR"
echo ""

# Configure: trim non-essential subsystems for embedded use.
#   ENABLE_SECURITY=OFF  — DDS Security plugin (needs OpenSSL); not needed
#                          for safety-island integration in v1.
#   ENABLE_SHM=OFF       — Iceoryx shared-memory transport; not in scope.
#   BUILD_IDLC=ON        — IDL compiler used by Phase 117.2 codegen.
#   BUILD_TESTING=OFF    — skip Cyclone's own gtest suite.
#   BUILD_EXAMPLES=OFF   — skip upstream examples.
#   BUILD_DDSPERF=OFF    — perf benchmarks not in scope.
cmake -S "$CYCLONEDDS_DIR" -B "$BUILD_DIR" \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_INSTALL_PREFIX="$INSTALL_DIR" \
    -DBUILD_SHARED_LIBS=ON \
    -DENABLE_SECURITY=OFF \
    -DENABLE_SHM=OFF \
    -DBUILD_IDLC=ON \
    -DBUILD_TESTING=OFF \
    -DBUILD_EXAMPLES=OFF \
    -DBUILD_DDSPERF=OFF

cmake --build "$BUILD_DIR" --parallel
cmake --install "$BUILD_DIR"

echo ""
echo "Build complete!"
echo "  ddsc:  $INSTALL_DIR/lib/libddsc.so"
echo "  idlc:  $INSTALL_DIR/bin/idlc"
echo "  cmake: $INSTALL_DIR/lib/cmake/CycloneDDS/CycloneDDSConfig.cmake"
