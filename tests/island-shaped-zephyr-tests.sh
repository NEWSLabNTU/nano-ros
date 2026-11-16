#!/usr/bin/env bash
# tests/island-shaped-zephyr-tests.sh -- phase-567 W4, the daily half.
#
# The island-shaped workspace's Zephyr image (`island_bringup:zephyr`,
# native_sim, Zephyr 3.7) states its pools above the derived counts in a
# Kconfig fragment, the shape issue 2618 refused after link. The merge queue
# has no west workspace (user ruling 2026-11-13), so it runs
# check-knob-delivery over a committed snapshot of this image's configured
# build dir (tests/fixtures/knob-delivery/island-shaped-zephyr). This script is
# what keeps that snapshot honest:
#
#   Z  `nros build island_bringup:zephyr` builds and links the image
#   K  check-knob-delivery over the LIVE build dir
#   S  the committed snapshot equals one cut from the live build dir, or this
#      fails and prints the command that regenerates it
#
# --regen rewrites the snapshot from the live build dir instead of S.
#
# PRECONDITIONS ARE HARD FAILURES (issue 1758): the in-tree `nros` on PATH and
# a provisioned Zephyr 3.7 west workspace (`just setup zephyr`).
#
# Usage: ./tests/island-shaped-zephyr-tests.sh [--regen]

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
WS="$PROJECT_ROOT/examples/workspaces/island-shaped"
FIXTURE="$PROJECT_ROOT/tests/fixtures/knob-delivery/island-shaped-zephyr"
SNAP="$PROJECT_ROOT/scripts/knob-delivery-fixture.py"

regen=0
[ "${1:-}" = "--regen" ] && regen=1

step() { printf '\n== island-shaped zephyr %s ==\n' "$*"; }
fail() {
    printf 'island-shaped-zephyr: FAIL -- %s\n' "$*" >&2
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

rm -rf "$WS/build" "$WS/generated"
cd "$WS" || fail "no workspace at $WS"

step "Z: build island_bringup:zephyr"
nros build island_bringup:zephyr ||
    fail "the Zephyr build refused the workspace (see the output above)"

# The west build dir: the one configured nano-ros Zephyr build under build/.
# shellcheck source=../scripts/lib/grep-q.sh
source "$PROJECT_ROOT/scripts/lib/grep-q.sh"
mapfile -t caches < <(find "$WS/build" -name CMakeCache.txt)
wait $! || fail "could not list the CMake caches under $WS/build"
dirs=()
for cache in "${caches[@]}"; do
    d="$(dirname "$cache")"
    [ -f "$d/zephyr/.config" ] || continue
    nros_grep_q '^NROS_RESOLVED_' "$cache"
    case $? in
        0) dirs+=("$d") ;;
        1) ;;
        *) fail "could not read $cache" ;;
    esac
done
[ "${#dirs[@]}" -eq 1 ] ||
    fail "expected one configured Zephyr build dir under $WS/build, found ${#dirs[@]}: ${dirs[*]:-none}"
bdir="${dirs[0]}"
printf 'build dir: %s\n' "$bdir"

step "K: knob delivery over the live build dir"
python3 "$PROJECT_ROOT/scripts/check-knob-delivery.py" "$bdir" ||
    fail "check-knob-delivery refused the live build dir $bdir"

if [ "$regen" -eq 1 ]; then
    step "S: regenerate the snapshot"
    python3 "$SNAP" extract "$bdir" "$FIXTURE" || fail "could not write $FIXTURE"
    printf '\nisland-shaped-zephyr: snapshot rewritten; commit %s\n' "${FIXTURE#"$PROJECT_ROOT"/}"
    exit 0
fi

step "S: the committed snapshot matches the live build dir"
python3 "$SNAP" diff "$bdir" "$FIXTURE" ||
    fail "the snapshot the merge queue checks is stale"

printf '\nisland-shaped-zephyr: every step held.\n'
