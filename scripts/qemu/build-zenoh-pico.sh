#!/bin/bash
# Build zenoh-pico for ARM Cortex-M3 (QEMU mps2-an385)
#
# This script cross-compiles zenoh-pico as a static library for bare-metal
# ARM Cortex-M3 targets, using our smoltcp platform layer.
#
# Usage:
#   ./scripts/qemu/build-zenoh-pico.sh [--clean]
#
# Output:
#   build/qemu-zenoh-pico/libzenohpico.a
#
# Prerequisites:
#   - arm-none-eabi-gcc toolchain
#   - cmake

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="$REPO_ROOT/build/qemu-zenoh-pico"
ZENOH_PICO_DIR="$REPO_ROOT/packages/transport/zenoh-pico-shim-sys/zenoh-pico"
PLATFORM_DIR="$REPO_ROOT/packages/transport/zenoh-pico-shim-sys/c/platform_smoltcp"
TOOLCHAIN_FILE="$REPO_ROOT/cmake/arm-none-eabi-cortex-m3.cmake"

# Parse arguments
if [ "$1" = "--clean" ]; then
    echo "Cleaning build directory..."
    rm -rf "$BUILD_DIR"
    echo "Done."
    exit 0
fi

# Check prerequisites
if ! command -v arm-none-eabi-gcc &>/dev/null; then
    echo "Error: arm-none-eabi-gcc not found"
    echo "Install with: sudo apt install gcc-arm-none-eabi"
    exit 1
fi

if ! command -v cmake &>/dev/null; then
    echo "Error: cmake not found"
    echo "Install with: sudo apt install cmake"
    exit 1
fi

# Check zenoh-pico submodule
if [ ! -d "$ZENOH_PICO_DIR/include" ]; then
    echo "Error: zenoh-pico submodule not found"
    echo "Run: git submodule update --init"
    exit 1
fi

echo "Building zenoh-pico for ARM Cortex-M3..."

# Create build directory
mkdir -p "$BUILD_DIR"

# Generate version header
VERSION=$(cat "$ZENOH_PICO_DIR/version.txt" 2>/dev/null || echo "0.11.0")
MAJOR=$(echo "$VERSION" | cut -d. -f1)
MINOR=$(echo "$VERSION" | cut -d. -f2)
PATCH=$(echo "$VERSION" | cut -d. -f3)

mkdir -p "$BUILD_DIR/include"
cat > "$BUILD_DIR/include/zenoh-pico.h" << EOF
// Auto-generated version header for zenoh-pico
#ifndef ZENOH_PICO_H
#define ZENOH_PICO_H

#define ZENOH_PICO "$VERSION"
#define ZENOH_PICO_MAJOR $MAJOR
#define ZENOH_PICO_MINOR $MINOR
#define ZENOH_PICO_PATCH $PATCH
#define ZENOH_PICO_TWEAK 0

#include "zenoh-pico/api.h"

#endif // ZENOH_PICO_H
EOF

# Collect zenoh-pico source files (excluding platform-specific code)
SOURCES=""
for dir in api collections link net protocol session transport utils; do
    for f in $(find "$ZENOH_PICO_DIR/src/$dir" -name "*.c" 2>/dev/null); do
        SOURCES="$SOURCES $f"
    done
done

# Add common system sources
for f in $(find "$ZENOH_PICO_DIR/src/system/common" -name "*.c" 2>/dev/null); do
    SOURCES="$SOURCES $f"
done

# Add our platform layer
SOURCES="$SOURCES $PLATFORM_DIR/system.c"
SOURCES="$SOURCES $PLATFORM_DIR/network.c"

# Add zenoh shim (high-level API wrapper)
SHIM_DIR="$REPO_ROOT/packages/transport/zenoh-pico-shim-sys/c/shim"
SOURCES="$SOURCES $SHIM_DIR/zenoh_shim.c"

# Compiler flags
CFLAGS="-mcpu=cortex-m3 -mthumb"
CFLAGS="$CFLAGS -Os -g"
CFLAGS="$CFLAGS -ffunction-sections -fdata-sections"
CFLAGS="$CFLAGS -fno-common -fno-exceptions"
CFLAGS="$CFLAGS -Wall -Wextra"

# Include paths
INCLUDES="-I$ZENOH_PICO_DIR/include"
INCLUDES="$INCLUDES -I$BUILD_DIR/include"
INCLUDES="$INCLUDES -I$PLATFORM_DIR"
INCLUDES="$INCLUDES -I$REPO_ROOT/packages/transport/zenoh-pico-shim-sys/c/include"

# Platform defines for smoltcp backend
DEFINES="-DZENOH_GENERIC"
DEFINES="$DEFINES -DZENOH_SHIM_SMOLTCP"
DEFINES="$DEFINES -DZ_FEATURE_MULTI_THREAD=0"
DEFINES="$DEFINES -DZ_FEATURE_LINK_TCP=1"
DEFINES="$DEFINES -DZ_FEATURE_LINK_UDP_MULTICAST=0"
DEFINES="$DEFINES -DZ_FEATURE_LINK_UDP_UNICAST=0"
DEFINES="$DEFINES -DZ_FEATURE_SCOUTING_UDP=0"
DEFINES="$DEFINES -DZ_FEATURE_LINK_SERIAL=0"
DEFINES="$DEFINES -DZ_FEATURE_LINK_WS=0"
DEFINES="$DEFINES -DZ_FEATURE_LINK_BLUETOOTH=0"
DEFINES="$DEFINES -DZ_FEATURE_RAWETH_TRANSPORT=0"
# Z_FEATURE_LOCAL_SUBSCRIBER is set in zenoh_generic_config.h
DEFINES="$DEFINES -DZENOH_DEBUG=0"

# Compile each source file
OBJECTS=""
count=0
for src in $SOURCES; do
    basename=$(basename "$src" .c)
    # Handle name collisions by using full path hash
    objname=$(echo "$src" | md5sum | cut -c1-8)_${basename}.o
    obj="$BUILD_DIR/$objname"

    arm-none-eabi-gcc $CFLAGS $INCLUDES $DEFINES -c "$src" -o "$obj"
    OBJECTS="$OBJECTS $obj"
    count=$((count + 1))
done

# Create static library
arm-none-eabi-ar rcs "$BUILD_DIR/libzenohpico.a" $OBJECTS

echo "Built $count sources → $BUILD_DIR/libzenohpico.a ($(stat -c%s "$BUILD_DIR/libzenohpico.a" | numfmt --to=iec-i --suffix=B))"
