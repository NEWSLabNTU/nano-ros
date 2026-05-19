#!/usr/bin/env bash
# scripts/zephyr/cargo-features-patch.sh
#
# Phase 168.1 — patch zephyr-lang-rust's
# `modules/lang/rust/CMakeLists.txt` to honor a CMake-set
# `EXTRA_CARGO_ARGS` variable, so per-example CMakeLists.txt can
# inject `--no-default-features --features rmw-<x>` based on the
# Kconfig RMW choice (CONFIG_NROS_RMW_<X>=y).
#
# Upstream has TODOs at lines 200-205 and 246-249 noting the
# missing pass-through — this patch fills the gap.
#
# Idempotent: detects prior application via grep and skips.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NANO_ROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
IN_TREE_WORKSPACE="$NANO_ROS_ROOT/zephyr-workspace"
LEGACY_WORKSPACE="$(cd "$NANO_ROS_ROOT/.." && pwd)/nano-ros-workspace"

if [ -n "${1:-}" ]; then
    WORKSPACE="$1"
elif [ -n "${NROS_ZEPHYR_WORKSPACE:-}" ]; then
    WORKSPACE="$NROS_ZEPHYR_WORKSPACE"
elif [ -d "$IN_TREE_WORKSPACE/zephyr" ]; then
    WORKSPACE="$IN_TREE_WORKSPACE"
else
    WORKSPACE="$LEGACY_WORKSPACE"
fi

CMAKE_FILE="$WORKSPACE/modules/lang/rust/CMakeLists.txt"
if [ ! -f "$CMAKE_FILE" ]; then
    echo "ERROR: $CMAKE_FILE missing" >&2
    exit 1
fi

if grep -q "nano-ros: EXTRA_CARGO_ARGS pass-through" "$CMAKE_FILE"; then
    echo "[cargo-features-patch] already applied to $CMAKE_FILE"
    exit 0
fi

# Inject ${EXTRA_CARGO_ARGS} immediately after every line containing
# only `${rust_build_type_arg}`. There are two such lines: cargo build
# (~199) and cargo doc (~243). awk handles both in one pass.
TMP="$(mktemp)"
awk '
{
    print
    if ($0 ~ /^[[:space:]]+\$\{rust_build_type_arg\}[[:space:]]*$/) {
        print ""
        print "      # nano-ros: EXTRA_CARGO_ARGS pass-through (Phase 168.1)."
        print "      # Honors CMakeLists.txt `set(EXTRA_CARGO_ARGS ...)` set"
        print "      # before `rust_cargo_application()`."
        print "      ${EXTRA_CARGO_ARGS}"
    }
}
' "$CMAKE_FILE" > "$TMP"

mv "$TMP" "$CMAKE_FILE"
echo "[cargo-features-patch] patched $CMAKE_FILE"
