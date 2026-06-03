#!/usr/bin/env bash
# Phase 212.K.7.8 — alloc-free audit for `nros-rmw-cyclonedds`.
#
# Builds the crate against the bare-metal `thumbv7m-none-eabi`
# target with `--no-default-features` (no `std`, no `bridge-stub`),
# then greps the resulting rlib for any symbol from Rust's `alloc`
# crate or the global allocator shims (`__rust_alloc*`). The K.7
# contract is **zero** alloc symbols in this configuration; any hit
# fails the audit.
#
# Invoked from `tests/bare_metal_link.rs::bare_metal_no_alloc_symbols`
# but also standalone-runnable for hand checks:
#
#   bash packages/dds/nros-rmw-cyclonedds/tests/alloc_free_audit.sh
#
# Exit codes:
#   0  — clean (no alloc symbols)
#   1  — alloc symbols found OR toolchain/`nm` missing
#   2  — internal usage error
set -euo pipefail

TARGET="thumbv7m-none-eabi"

# Resolve workspace root from this script's location:
# packages/dds/nros-rmw-cyclonedds/tests/ → up 4 = workspace root.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
WORKSPACE_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"

cd "$WORKSPACE_ROOT"

if ! command -v nm >/dev/null 2>&1; then
    echo "FAIL: \`nm\` not on PATH; cannot audit symbols." >&2
    exit 1
fi

if ! rustup target list --installed 2>/dev/null | grep -qx "$TARGET"; then
    echo "FAIL: $TARGET target not installed. Run: rustup target add $TARGET" >&2
    exit 1
fi

echo "→ Building nros-rmw-cyclonedds for $TARGET (--no-default-features)…"
cargo build -p nros-rmw-cyclonedds --no-default-features --target "$TARGET" >&2

DEPS_DIR="target/$TARGET/debug/deps"
RLIB="$(find "$DEPS_DIR" -maxdepth 1 -name 'libnros_rmw_cyclonedds*.rlib' -printf '%T@ %p\n' \
        | sort -nr | head -1 | cut -d' ' -f2-)"

if [[ -z "${RLIB:-}" ]]; then
    echo "FAIL: no libnros_rmw_cyclonedds*.rlib found under $DEPS_DIR" >&2
    exit 1
fi
echo "→ Auditing $RLIB"

# Grep the rlib's symbol table for:
#   * `_ZN5alloc…`        — mangled Rust `alloc::*` paths
#   * `__rust_alloc`,     — the global allocator entry points
#     `__rust_dealloc`,
#     `__rust_realloc`,
#     `__rust_alloc_zeroed`
#
# `nm` against a multi-object rlib returns one section per object; we
# concatenate the whole stream and grep. `|| true` keeps the pipeline
# from tripping `set -e` when there are zero hits (grep exits 1).
HITS="$(nm "$RLIB" 2>/dev/null \
        | grep -E '(_ZN5alloc[0-9])|(^|[[:space:]])__rust_(alloc|dealloc|realloc|alloc_zeroed)([[:space:]]|$)' \
        || true)"

if [[ -n "$HITS" ]]; then
    echo "FAIL: alloc symbols leaked into $RLIB:" >&2
    echo "$HITS" >&2
    exit 1
fi

echo "OK: no alloc symbols in $(basename "$RLIB")"
