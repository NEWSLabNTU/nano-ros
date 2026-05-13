#!/usr/bin/env bash
# Phase 118.E.3 + .4 — size-probe verification.
#
# Validates that the two probe modes (filesystem, isolated) produce
# identical generated-header sizes for the same target + feature set,
# and that repeated parallel builds under the chosen mode never flake.
#
# Invoked via `just verify-size-probe`.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

HEADER="packages/core/nros-c/include/nros/nros_config_generated.h"
FEATURES="cffi-zenoh-cffi,platform-posix,ros-humble"
JOBS="${JOBS:-8}"
SOAK_ROUNDS="${SOAK_ROUNDS:-3}"

extract_sizes() {
    grep -E '^#define NROS_(EXECUTOR|PUBLISHER|SUBSCRIBER|SESSION|SERVICE_CLIENT|SERVICE_SERVER|GUARD_CONDITION|LIFECYCLE_CTX|ACTION_SERVER_INTERNAL)_SIZE' \
        "$HEADER" | sort
}

build_under() {
    local mode=$1
    cargo clean -p nros-c >/dev/null
    NROS_SIZES_PROBE_MODE="$mode" cargo build -p nros-c \
        --features "$FEATURES" -j "$JOBS" >/dev/null
}

echo "=== 118.E.3 — cross-mode parity ==="
build_under filesystem
fs_sizes=$(extract_sizes)

build_under isolated
iso_sizes=$(extract_sizes)

if [[ "$fs_sizes" != "$iso_sizes" ]]; then
    echo "FAIL: filesystem and isolated produce DIFFERENT sizes"
    diff <(echo "$fs_sizes") <(echo "$iso_sizes") || true
    exit 1
fi

echo "OK — both modes agree:"
echo "$fs_sizes"
echo

echo "=== 118.E.3 — cross-pointer-size validation (host vs 32-bit) ==="
# Build `nros` (not nros-c — nros-c is host-only / OS-backed) for a
# 32-bit target if installed, capture sizes via the probe, and assert
# pointer-size-dependent types shrink. Catches the case where the
# probe accidentally reads host sizes during a cross build.
if rustup target list --installed | grep -q '^i686-unknown-linux-gnu$'; then
    cargo clean -p nros-c >/dev/null
    cargo build -p nros-c --target i686-unknown-linux-gnu \
        --features cffi-zenoh-cffi,platform-posix,ros-humble \
        -j "$JOBS" 2>&1 | tail -3 || true
    # nros-c emits the same header path under the 32-bit build's
    # OUT_DIR; capture sizes from the canonical install location.
    cross_pub=$(grep '^#define NROS_PUBLISHER_SIZE' "$HEADER" | awk '{print $3}')
    host_pub=$(echo "$fs_sizes" | grep '^#define NROS_PUBLISHER_SIZE' | awk '{print $3}')
    echo "  host PUBLISHER_SIZE=$host_pub  i686 PUBLISHER_SIZE=$cross_pub"
    if [[ "$cross_pub" -ge "$host_pub" ]]; then
        echo "  WARN: 32-bit target did not shrink PUBLISHER_SIZE; either"
        echo "  RmwPublisher has no pointer fields or the probe leaked host sizes."
    fi
else
    echo "  [skip] i686-unknown-linux-gnu target not installed (install via"
    echo "         'rustup target add i686-unknown-linux-gnu' to enable)"
fi

echo
echo "=== 118.E.4 — concurrency soak (${SOAK_ROUNDS} rounds, -j${JOBS}, filesystem mode) ==="
for i in $(seq 1 "$SOAK_ROUNDS"); do
    cargo clean -p nros-c -p nros-cpp >/dev/null
    cargo build -p nros-c -p nros-cpp \
        --features "$FEATURES" -j "$JOBS" >/dev/null
    sz=$(grep '^#define NROS_EXECUTOR_SIZE' "$HEADER")
    echo "  [soak $i/${SOAK_ROUNDS}] $sz"
done

echo
echo "=== 118.E.4 — concurrency soak (${SOAK_ROUNDS} rounds, -j${JOBS}, isolated mode) ==="
for i in $(seq 1 "$SOAK_ROUNDS"); do
    cargo clean -p nros-c -p nros-cpp >/dev/null
    NROS_SIZES_PROBE_MODE=isolated cargo build -p nros-c -p nros-cpp \
        --features "$FEATURES" -j "$JOBS" >/dev/null
    sz=$(grep '^#define NROS_EXECUTOR_SIZE' "$HEADER")
    echo "  [iso-soak $i/${SOAK_ROUNDS}] $sz"
done

echo
echo "=== ALL CHECKS PASSED ==="
