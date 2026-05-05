#!/bin/bash
# tests/copy-out-example-test.sh
#
# Phase 112.E.3 acceptance test: each `examples/<plat>/<lang>/<rmw>/<usecase>`
# tree must build standalone after being copied outside the project root,
# given only `CMAKE_PREFIX_PATH=<install-prefix>` (and a per-platform
# `-DCMAKE_TOOLCHAIN_FILE=<install-prefix>/share/nano_ros/toolchains/...`).
#
# The example must NOT reach back into the project tree — no `../../../`,
# no walk-up heuristics. The shipped `<plat>-support.cmake` resolves
# every asset (board config, driver, startup.c) relative to the install
# prefix.
#
# Usage:
#   ./tests/copy-out-example-test.sh                        # default: freertos talker
#   ./tests/copy-out-example-test.sh --example c/zenoh/talker --platform qemu-arm-freertos
#
# Exit codes:
#   0 — copy-out built
#   1 — failure (configure or build error, or example escapes project tree)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

PLATFORM="qemu-arm-freertos"
EXAMPLE_REL="c/zenoh/talker"
TOOLCHAIN_NAME="arm-freertos-armcm3.cmake"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --platform)  PLATFORM="$2"; shift 2 ;;
        --example)   EXAMPLE_REL="$2"; shift 2 ;;
        --toolchain) TOOLCHAIN_NAME="$2"; shift 2 ;;
        -h|--help)
            head -n 20 "$0"; exit 0 ;;
        *) echo "unknown arg: $1" >&2; exit 1 ;;
    esac
done

EXAMPLE_DIR="$PROJECT_ROOT/examples/$PLATFORM/$EXAMPLE_REL"
INSTALL_PREFIX="$PROJECT_ROOT/build/install"
TOOLCHAIN_FILE="$INSTALL_PREFIX/share/nano_ros/toolchains/$TOOLCHAIN_NAME"

if [ ! -d "$EXAMPLE_DIR" ]; then
    echo "[FAIL] example not found: $EXAMPLE_DIR" >&2
    exit 1
fi
if [ ! -d "$INSTALL_PREFIX" ]; then
    echo "[SKIPPED] install prefix not present at $INSTALL_PREFIX." >&2
    echo "          run 'just freertos install' first" >&2
    exit 0
fi
if [ ! -f "$TOOLCHAIN_FILE" ]; then
    echo "[FAIL] toolchain not installed: $TOOLCHAIN_FILE" >&2
    exit 1
fi

WORK_DIR="$(mktemp -d /tmp/nros-copy-out-XXXXXX)"
cleanup() { rm -rf "$WORK_DIR"; }
trap cleanup EXIT

DEST="$WORK_DIR/example"
mkdir -p "$DEST"
# Use rsync with explicit excludes — `cp -r` would drag stale build/
# directories that pin the original source path in CMakeCache.txt.
rsync -a --exclude='/build' --exclude='/target' --exclude='/.cargo/registry' \
    "$EXAMPLE_DIR/" "$DEST/"

# Sanity: copied tree must NOT have `../../../` escapes into the project.
if grep -rE '\.\./\.\./\.\.' "$DEST" --include='*.cmake' --include='CMakeLists.txt' >/tmp/copy-out-escapes.log 2>&1; then
    echo "[FAIL] example references ../../../ — not self-contained:" >&2
    cat /tmp/copy-out-escapes.log >&2
    exit 1
fi

echo "=== Configuring copy-out example at $DEST ==="
cmake -S "$DEST" -B "$DEST/build" \
    -DCMAKE_PREFIX_PATH="$INSTALL_PREFIX" \
    -DCMAKE_TOOLCHAIN_FILE="$TOOLCHAIN_FILE" \
    -DCMAKE_BUILD_TYPE=Release

echo "=== Building copy-out example ==="
cmake --build "$DEST/build" --parallel

# Verify an executable was produced.
if ! find "$DEST/build" -maxdepth 2 -type f -executable -size +1k -print | grep -q .; then
    echo "[FAIL] no executable produced under $DEST/build" >&2
    exit 1
fi

echo "[PASS] copy-out built: $PLATFORM / $EXAMPLE_REL"
