#!/usr/bin/env bash
# scripts/zephyr/rust-cargo-extra-args-patch.sh
#
# Phase 200.1 — make zephyr-lang-rust's `rust_cargo_application()` honor the
# app-provided `EXTRA_CARGO_ARGS` variable for BOTH the `cargo build` and the
# `cargo clippy` invocations.
#
# Background. nano-ros's Zephyr Rust examples are multi-RMW: a single example
# crate selects its RMW backend (and the matching `nros-rmw-*` optional dep) via
# a Cargo feature, chosen per build from the Kconfig overlay
# (`prj-<rmw>.conf` → `CONFIG_NROS_RMW_<X>`):
#
#   examples/zephyr/rust/<ex>/CMakeLists.txt:
#     if(CONFIG_NROS_RMW_ZENOH)      set(EXTRA_CARGO_ARGS --no-default-features --features rmw-zenoh)
#     elseif(CONFIG_NROS_RMW_XRCE)   set(EXTRA_CARGO_ARGS --no-default-features --features rmw-xrce)
#     elseif(CONFIG_NROS_RMW_CYCLONEDDS) set(EXTRA_CARGO_ARGS --no-default-features --features rmw-cyclonedds)
#     rust_cargo_application()
#
# The zephyr-lang-rust pin chosen in Phase 199.2 (west.yml) ships a
# `rust_cargo_application()` that hard-codes `CARGO_ARGS build` and never reads
# `EXTRA_CARGO_ARGS` (upstream samples have a single fixed feature set, so they
# never needed it). The consequence: every Rust Zephyr build ignores the RMW
# feature selection and compiles the crate's *default* features
# (`default = ["rmw-zenoh"]`). A zenoh build happens to match, but an xrce /
# cyclonedds build then compiles the full `nros_rmw_zenoh` shim while the CMake
# side compiles the xrce / cyclone C — so the link fails with undefined
# `zpico_open` / `zpico_spin_once` / `zpico_declare_publisher` / … (the entire
# zenoh data path), which looked like a "zpico-link gap" but is really a dropped
# feature selection.
#
# This patch appends `${EXTRA_CARGO_ARGS}` to the `librustapp` build and to the
# `clippy` lint command inside `rust_cargo_application()`, so the example's
# per-RMW `--no-default-features --features rmw-<x>` actually reaches cargo. When
# `EXTRA_CARGO_ARGS` is unset (upstream samples), the expansion is empty → no
# behavior change.
#
# Idempotent: detects prior application via a marker comment and skips.

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
    echo "[rust-cargo-extra-args-patch] $CMAKE_FILE missing (rust module not present); skipping"
    exit 0
fi

if grep -q "nano-ros: EXTRA_CARGO_ARGS" "$CMAKE_FILE"; then
    echo "[rust-cargo-extra-args-patch] already applied to $CMAKE_FILE"
    exit 0
fi

python3 - "$CMAKE_FILE" <<'PYEOF'
import sys
from pathlib import Path

path = Path(sys.argv[1])
src = path.read_text()

build_old = "    CARGO_ARGS build\n    COMMENT \"Building Rust application\""
build_new = (
    "    # nano-ros: EXTRA_CARGO_ARGS — forward the example's per-RMW Cargo\n"
    "    # feature selection (Phase 200.1) into the build invocation.\n"
    "    CARGO_ARGS build ${EXTRA_CARGO_ARGS}\n"
    "    COMMENT \"Building Rust application\""
)

clippy_old = (
    "    CARGO_ARGS\n"
    "      clippy\n"
    "      --\n"
)
clippy_new = (
    "    # nano-ros: EXTRA_CARGO_ARGS — lint the same feature set that is built.\n"
    "    CARGO_ARGS\n"
    "      clippy\n"
    "      ${EXTRA_CARGO_ARGS}\n"
    "      --\n"
)

if build_old not in src:
    print(f"ERROR: librustapp CARGO_ARGS block not found in {path}", file=sys.stderr)
    sys.exit(1)
src = src.replace(build_old, build_new, 1)

if clippy_old in src:
    src = src.replace(clippy_old, clippy_new, 1)
else:
    print(f"WARNING: clippy CARGO_ARGS block not found in {path}; build patched only",
          file=sys.stderr)

path.write_text(src)
PYEOF

echo "[rust-cargo-extra-args-patch] patched $CMAKE_FILE"
