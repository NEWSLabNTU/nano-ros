#!/usr/bin/env bash
# Phase 180.B item 5 — copy-out CI check.
#
# Proves the copy-out promise: an `examples/zephyr/<lang>/<example>` dir,
# copied OUT of the nano-ros repo tree, still builds against the nano-ros
# Zephyr module. The copied dir references nano-ros ONLY via the Zephyr
# module (west manifest -> `nano-ros/zephyr/module.yml`), never via a
# project-tree walk-up / `add_subdirectory(<repo-root>)`. If it builds
# from outside the tree, the module-only consumption contract holds.
#
# Mechanics mirror `just zephyr build-one` on the 4.4 line:
#   - west invoked through the workspace Python 3.12 venv interpreter
#   - host toolchain (ZEPHYR_TOOLCHAIN_VARIANT=host; native_sim = host gcc)
#   - host nros-codegen passed via -D_NANO_ROS_CODEGEN_TOOL
#   - the version-aware NSOS line overlay appended to CONF_FILE
#   - NROS_<PKG>_DIR env defaults for the ROS interface packages
#
# PASS iff the build reaches `<build>/zephyr/zephyr.elf`.
# Idempotent; cleans the temp dir on exit (success OR failure).
#
# Usage:
#   scripts/zephyr/check-copy-out.sh [<example>] [<rmw>] [<board>]
# Defaults: c/talker zenoh native_sim/native/64
#
# Env knobs (all optional):
#   NROS_ZEPHYR_WORKSPACE   override the 4.4 workspace path
#   NROS_COPY_OUT_KEEP=1    keep the temp dir (debugging)

set -euo pipefail

EXAMPLE="${1:-c/talker}"
RMW="${2:-zenoh}"
BOARD="${3:-native_sim/native/64}"

# Resolve the nano-ros repo root from this script's location, so the
# script itself works copied/symlinked anywhere. (The thing being PROVEN
# copy-out clean is the *example*, not this driver script.)
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
NROS_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
cd "$NROS_ROOT"

# shellcheck source=/dev/null
source scripts/build/cargo.sh

# ROS interface package dirs (same defaults as build-one). Override per
# package if your ROS install lives elsewhere.
export NROS_STD_MSGS_DIR="${NROS_STD_MSGS_DIR:-/opt/ros/humble/share/std_msgs}"
export NROS_EXAMPLE_INTERFACES_DIR="${NROS_EXAMPLE_INTERFACES_DIR:-/opt/ros/humble/share/example_interfaces}"
export NROS_BUILTIN_INTERFACES_DIR="${NROS_BUILTIN_INTERFACES_DIR:-/opt/ros/humble/share/builtin_interfaces}"
export NROS_UNIQUE_IDENTIFIER_MSGS_DIR="${NROS_UNIQUE_IDENTIFIER_MSGS_DIR:-/opt/ros/humble/share/unique_identifier_msgs}"
export NROS_ACTION_MSGS_DIR="${NROS_ACTION_MSGS_DIR:-/opt/ros/humble/share/action_msgs}"

# This check exercises the 4.4 line (latest rolling) — that is the line
# with the module-contributed snippets + the Phase 180 module mechanism.
WORKSPACE_DEFAULT="../nano-ros-workspace-4.4"
WORKSPACE="${NROS_ZEPHYR_WORKSPACE:-$WORKSPACE_DEFAULT}"
workspace="$(realpath "$WORKSPACE" 2>/dev/null || echo "")"
if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
    echo "FAIL: Zephyr 4.4 workspace not set up at $WORKSPACE"
    echo "  run: NROS_ZEPHYR_VERSION=4.4 just zephyr setup"
    exit 1
fi

venvbin="$workspace/.venv312/bin"
if [ ! -x "$venvbin/python" ]; then
    echo "FAIL: 4.4 Python 3.12 venv missing at $venvbin/python"
    echo "  run: NROS_ZEPHYR_VERSION=4.4 just zephyr setup"
    exit 1
fi

# Source example inside the repo tree.
src="$NROS_ROOT/examples/zephyr/$EXAMPLE"
[ -d "$src" ] || { echo "FAIL: no example at $src"; exit 1; }

# Host codegen tool — the example's nros_generate_interfaces() needs the
# installed `nros` (Phase 195.D: resolved from $NROS_CLI / PATH / ~/.nros).
nros_cargo_ensure_codegen_c
codegen_tool="$(nros_cargo_codegen_c_bin)"
[ -x "$codegen_tool" ] || { echo "FAIL: nros codegen tool not found ($codegen_tool); run scripts/install-nros.sh"; exit 1; }

# Version-aware NSOS line overlay (4.4 symbol names).
line_overlay="$NROS_ROOT/cmake/zephyr/native-sim-line-4.4.conf"
[ -f "$line_overlay" ] || { echo "FAIL: missing $line_overlay"; exit 1; }

# --- Copy the example OUT of the repo tree --------------------------------
tmp_root="$(mktemp -d "${TMPDIR:-/tmp}/nros-copy-out.XXXXXX")"
ex_leaf="$(basename "$EXAMPLE")"
copied="$tmp_root/$ex_leaf"

cleanup() {
    if [ "${NROS_COPY_OUT_KEEP:-0}" = "1" ]; then
        echo "[copy-out] keeping temp dir: $tmp_root"
    else
        rm -rf "$tmp_root"
    fi
}
trap cleanup EXIT

cp -a "$src" "$copied"
# Strip any stale build artifacts / generated dirs that a `cp -a` may drag
# along, so we prove a clean-tree build.
rm -rf "$copied"/build* "$copied"/generated

# Sanity: the copied dir must not reach back into the repo tree. The
# copy-out contract is that nano-ros is consumed ONLY via the Zephyr
# module — so the example CMake must not walk up to the repo root.
if grep -RInE 'add_subdirectory\([^)]*(\.\./){2,}|NANO_ROS_ROOT|\$\{CMAKE_SOURCE_DIR\}/\.\.' "$copied/CMakeLists.txt" >/dev/null 2>&1; then
    echo "FAIL: copied example CMakeLists walks up the repo tree — not copy-out clean"
    exit 1
fi

echo "[copy-out] repo root : $NROS_ROOT"
echo "[copy-out] example   : examples/zephyr/$EXAMPLE  ($RMW, $BOARD)"
echo "[copy-out] copied to : $copied  (OUTSIDE repo tree)"
echo "[copy-out] workspace : $workspace (4.4 line)"

# --- Build the copied example via the nano-ros Zephyr module --------------
conf="prj.conf;prj-$RMW.conf;$line_overlay"
bd="$tmp_root/build"

make_bin="$NROS_ROOT/third-party/make/make"
[ -x "$make_bin" ] || make_bin="$(command -v make)"

export ZEPHYR_TOOLCHAIN_VARIANT=host
export PATH="$NROS_ROOT/third-party/make:$NROS_ROOT/third-party/ninja:$PATH"

echo "[copy-out] building (west via venv python) ..."
set +e
(
    cd "$workspace"
    "$(realpath "$venvbin")/python" -m west build \
        -b "$BOARD" -d "$bd" -p auto "$copied" -- \
        -DCONF_FILE="$conf" \
        -D_NANO_ROS_CODEGEN_TOOL="$codegen_tool" \
        -DMAKE="$make_bin"
)
rc=$?
set -e

elf="$bd/zephyr/zephyr.elf"
if [ "$rc" -eq 0 ] && [ -f "$elf" ]; then
    echo "Built: $elf"
    echo "PASS: copied-out example built from OUTSIDE the repo tree via the nano-ros Zephyr module."
    exit 0
fi

echo "FAIL: copy-out build did not reach zephyr.elf (rc=$rc)"
exit 1
