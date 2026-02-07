#!/bin/bash
# Build zenoh-pico for ESP32-C3 (RISC-V RV32IMC)
#
# This script cross-compiles zenoh-pico as a static library for bare-metal
# RISC-V RV32IMC targets (ESP32-C3), using our smoltcp platform layer.
#
# Usage:
#   ./scripts/esp32/build-zenoh-pico.sh [--clean]
#
# Output:
#   build/esp32-zenoh-pico/libzenohpico.a
#
# Prerequisites:
#   - riscv64-unknown-elf-gcc (or riscv32-esp-elf-gcc from ESP-IDF)
#   - picolibc-riscv64-unknown-elf (C standard library headers)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
BUILD_DIR="$REPO_ROOT/build/esp32-zenoh-pico"
ZENOH_PICO_DIR="$REPO_ROOT/crates/zenoh-pico-shim-sys/zenoh-pico"
PLATFORM_DIR="$REPO_ROOT/crates/zenoh-pico-shim-sys/c/platform_smoltcp"

# Parse arguments
if [ "$1" = "--clean" ]; then
    echo "Cleaning build directory..."
    rm -rf "$BUILD_DIR"
    echo "Done."
    exit 0
fi

# Detect RISC-V toolchain
CC=""
AR=""
if command -v riscv64-unknown-elf-gcc &>/dev/null; then
    CC="riscv64-unknown-elf-gcc"
    AR="riscv64-unknown-elf-ar"
elif command -v riscv32-esp-elf-gcc &>/dev/null; then
    CC="riscv32-esp-elf-gcc"
    AR="riscv32-esp-elf-ar"
else
    echo "Error: RISC-V GCC not found"
    echo "Install with: sudo apt install gcc-riscv64-unknown-elf"
    echo "Or use ESP-IDF toolchain: riscv32-esp-elf-gcc"
    exit 1
fi

echo "Using toolchain: $CC"

# Detect picolibc include path (needed for C standard library headers).
# We do NOT use --specs=picolibc.specs because it enables TLS (-ftls-model=local-exec)
# which makes errno a __thread variable accessed via the tp register. On bare-metal
# ESP32-C3, tp is never initialized (=0), causing null pointer crashes in strtoul etc.
# Instead, we add the include path directly and override errno to use our __errno() stub.
PICOLIBC_INCLUDES=""
PICOLIBC_SYSROOT=$($CC -march=rv32imc -mabi=ilp32 --specs=picolibc.specs -print-sysroot 2>/dev/null || true)
if [ -n "$PICOLIBC_SYSROOT" ] && [ -d "$PICOLIBC_SYSROOT/include" ]; then
    PICOLIBC_INCLUDES="-isystem $PICOLIBC_SYSROOT/include"
elif [ -d "/usr/lib/picolibc/riscv64-unknown-elf/include" ]; then
    PICOLIBC_INCLUDES="-isystem /usr/lib/picolibc/riscv64-unknown-elf/include"
fi

# Create a wrapper errno.h that shadows picolibc's TLS-based version.
# picolibc declares `extern __thread int errno` which uses the tp (thread pointer)
# register. On bare-metal ESP32-C3, tp is never initialized → null pointer crash.
# Our wrapper provides a plain `extern int errno` (backed by libc_stubs.rs).
mkdir -p "$BUILD_DIR/include"
cat > "$BUILD_DIR/include/errno.h" << 'ERRNO_EOF'
#ifndef _ERRNO_OVERRIDE_H
#define _ERRNO_OVERRIDE_H

/* Minimal errno.h for bare-metal RISC-V (no TLS).
 * Shadows picolibc's errno.h which uses __thread. */
extern int errno;

#define EPERM    1
#define ENOENT   2
#define EIO      5
#define ENOMEM  12
#define EACCES  13
#define EFAULT  14
#define EBUSY   16
#define EEXIST  17
#define EINVAL  22
#define ENOSPC  28
#define ERANGE  34
#define ENOSYS  88
#define ENOMSG  91
#define ENOTSUP 95
#define EADDRINUSE 98
#define EADDRNOTAVAIL 99
#define ENETUNREACH 101
#define ECONNABORTED 103
#define ECONNRESET 104
#define ENOBUFS 105
#define EISCONN 106
#define ENOTCONN 107
#define ETIMEDOUT 110
#define ECONNREFUSED 111
#define EALREADY 114
#define EINPROGRESS 115
#define EAGAIN  11
#define EWOULDBLOCK EAGAIN

#endif
ERRNO_EOF

# Check zenoh-pico submodule
if [ ! -d "$ZENOH_PICO_DIR/include" ]; then
    echo "Error: zenoh-pico submodule not found"
    echo "Run: git submodule update --init"
    exit 1
fi

echo "Building zenoh-pico for ESP32-C3 (RISC-V RV32IMC)..."

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
SHIM_DIR="$REPO_ROOT/crates/zenoh-pico-shim-sys/c/shim"
SOURCES="$SOURCES $SHIM_DIR/zenoh_shim.c"

# Compiler flags for ESP32-C3 (RISC-V RV32IMC)
# Use picolibc headers for standard library (stdint.h, stdlib.h, etc.) but our own
# errno.h shadows picolibc's TLS-based version. BUILD_DIR/include comes first in
# search order so our errno.h is found before picolibc's.
CFLAGS="-march=rv32imc -mabi=ilp32"
CFLAGS="$CFLAGS -isystem $BUILD_DIR/include $PICOLIBC_INCLUDES"
CFLAGS="$CFLAGS -Os -g"
CFLAGS="$CFLAGS -ffunction-sections -fdata-sections"
CFLAGS="$CFLAGS -fno-common -fno-exceptions"
CFLAGS="$CFLAGS -Wall -Wextra"

# Include paths
INCLUDES="-I$ZENOH_PICO_DIR/include"
INCLUDES="$INCLUDES -I$BUILD_DIR/include"
INCLUDES="$INCLUDES -I$PLATFORM_DIR"
INCLUDES="$INCLUDES -I$REPO_ROOT/crates/zenoh-pico-shim-sys/c/include"

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

    $CC $CFLAGS $INCLUDES $DEFINES -c "$src" -o "$obj"
    OBJECTS="$OBJECTS $obj"
    count=$((count + 1))
done

# Create static library
$AR rcs "$BUILD_DIR/libzenohpico.a" $OBJECTS

echo "Built $count sources → $BUILD_DIR/libzenohpico.a ($(stat -c%s "$BUILD_DIR/libzenohpico.a" | numfmt --to=iec-i --suffix=B))"
