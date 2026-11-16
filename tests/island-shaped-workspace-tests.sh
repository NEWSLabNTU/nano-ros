#!/usr/bin/env bash
# tests/island-shaped-workspace-tests.sh -- phase-567 W4.
#
# The island-shaped workspace (examples/workspaces/island-shaped), built the
# way the safety island builds, so a layout the island hit breaks the merge
# queue (`check-workspace-all`) and not a downstream build:
#
#   (K runs first: it needs no build.)
#   M  the model records both sibling-package param files (absolute and
#      $(dirname)-relative) as absolute inputs, and every consumer reads it
#      (issue 2615: model_gate refused every absolute input)
#   C  `nros configure --census island_bringup:freertos` builds the native
#      image of the C and Rust components, runs it and records the census
#   B  `nros build island_bringup:freertos`, the board build, with its pools
#      stated above the derived counts
#   K  check-knob-delivery over the committed snapshot of the Zephyr image's
#      configured build dir, whose Kconfig fragment states the pools above the
#      derived counts (issue 2618). The merge queue has no west workspace; the
#      daily Zephyr lane (`just check island-shaped-zephyr`) checks the live
#      build dir and fails when the snapshot differs from it.
#
# PRECONDITIONS ARE HARD FAILURES (issue 1758): the in-tree `nros` on PATH,
# arm-none-eabi-gcc and the FreeRTOS kernel + lwIP sources (`nros setup --tool
# arm-none-eabi-gcc --source freertos-kernel --source lwip`).
#
# Usage: ./tests/island-shaped-workspace-tests.sh
# Exit:  0 all steps held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WS="$PROJECT_ROOT/examples/workspaces/island-shaped"
FIXTURE="$PROJECT_ROOT/tests/fixtures/knob-delivery/island-shaped-zephyr"
MODEL="$WS/build/nros/models/island_bringup/system_model.yaml"

step() { printf '\n== island-shaped %s ==\n' "$*"; }
fail() {
    printf 'island-shaped-workspace: FAIL -- %s\n' "$*" >&2
    exit 1
}

# shellcheck source=scripts/build/cargo.sh
source "$PROJECT_ROOT/scripts/build/cargo.sh"
# issue 1850 -- this checkout's CLI. The cmake modules resolve `nros` off PATH,
# and another checkout's binary there would configure this tree with that
# checkout's codegen (issue 0363), so put this one first; and pin the cmake
# package search and the checkout the CLI reports against the same way.
NROS_BIN="$(nros_cli_bin --checkout-only)" || NROS_BIN=""
[ -n "$NROS_BIN" ] || fail "this checkout's nros CLI is not built (just setup-cli)"
export PATH="$(dirname "$NROS_BIN"):$PATH"
export nano_ros_ROOT="$PROJECT_ROOT"
export NROS_REPO_DIR="$PROJECT_ROOT"

# Buildless first, so a refusal here costs seconds.
step "K: knob delivery over the Zephyr build-dir snapshot"
python3 "$PROJECT_ROOT/scripts/check-knob-delivery.py" "$FIXTURE" ||
    fail "check-knob-delivery refused the island-shaped Zephyr image's snapshot ($FIXTURE)"

# A clean tree: a model or a stamp left by an earlier run must not decide
# whether this one passes.
rm -rf "$WS/build" "$WS/generated"
cd "$WS" || fail "no workspace at $WS"

step "C: configure + census (island_bringup:freertos)"
nros configure --census island_bringup:freertos ||
    fail "configure --census refused the workspace (see the output above)"

step "M: the sibling param files are absolute model inputs"
[ -f "$MODEL" ] || fail "no model at $MODEL"
n=$(grep -cE "^  - path: /.*/src/c_controller_pkg/config/.*\\.param\\.yaml$" "$MODEL")
[ "$n" -eq 2 ] ||
    fail "expected the 2 sibling param files as absolute inputs in $MODEL, found $n -- the workspace no longer has the issue-2615 shape"

step "B: board build (island_bringup:freertos, pools raised)"
nros build island_bringup:freertos ||
    fail "the board build refused the workspace (see the output above)"

printf '\nisland-shaped-workspace: every step held.\n'
