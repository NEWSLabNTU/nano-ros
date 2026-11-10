#!/usr/bin/env bash
# tests/zephyr-transient-overrides-tests.sh -- issue 2363.
#
# WHAT THIS MEASURES
#
# A knob override from the command line (`-DNROS_OVERRIDE_<ID>=<v>`, a fixture
# row's `west_overrides`) is TRANSIENT (RFC-0103 D2): it belongs to the
# configure that passed it. CMake caches every `-D`, so before issue 2363 a
# re-configure of the same build dir that no longer passed it still applied it,
# and only a wipe dropped it. The Zephyr module's hook now consumes each one
# (`_nros_image_consume_overrides`, zephyr/cmake/nros_image_kconfig.cmake).
#
#   A  a configure with -DNROS_OVERRIDE_X=7 -DNROS_OVERRIDE_Y=on sees both,
#      with their values
#   B  the cache holds neither afterwards
#   C  a re-configure of the SAME build dir with no -D sees none (the issue's
#      acceptance: "configure with an override, then without, and the second
#      configure reports none")
#   D  a third configure passing only Y sees only Y
#   E  REACH: the hook harvests through the function and reads no
#      `$CACHE{NROS_OVERRIDE_*}` itself
#   F  the fixture runner's override listing (`fixtures-manifest.py
#      west-overrides`) runs: it crashed with an `UnboundLocalError` from
#      2865cc77c5, and the runner read it through a process substitution, so
#      every row built with no override and nothing said so
#
# A probe project that includes the module file, so seconds; no Zephyr
# workspace. The end-to-end form (a real `west build --cmake-only` twice on one
# build dir) is recorded in the issue. PRECONDITIONS ARE HARD FAILURES (issue
# 1758): no cmake, no verdict, red.
#
# Usage: ./tests/zephyr-transient-overrides-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=../scripts/lib/grep-q.sh
source "$PROJECT_ROOT/scripts/lib/grep-q.sh"
MODULE="$PROJECT_ROOT/zephyr/cmake/nros_image_kconfig.cmake"

FAILURES=0
fail() {
    echo "FAIL: $*" >&2
    FAILURES=$((FAILURES + 1))
}

command -v cmake >/dev/null 2>&1 || { echo "FAIL: cmake not found" >&2; exit 1; }

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
mkdir -p "$WORK/src"
printf '%s\n' \
    'cmake_minimum_required(VERSION 3.22)' \
    'project(probe NONE)' \
    'include("${MODULE}")' \
    '_nros_image_consume_overrides(_ids)' \
    'foreach(_id IN LISTS _ids)' \
    '    message(STATUS "PROBE_OVERRIDE ${_id}=${_override_val_${_id}}")' \
    'endforeach()' \
    'if(NOT _ids)' \
    '    message(STATUS "PROBE_OVERRIDE none")' \
    'endif()' \
    > "$WORK/src/CMakeLists.txt"

configure() {
    cmake -S "$WORK/src" -B "$WORK/b" "-DMODULE=$MODULE" "$@" 2>&1 | sed -n 's/.*PROBE_OVERRIDE //p' | sort
}

seen="$(configure -DNROS_OVERRIDE_X=7 -DNROS_OVERRIDE_Y=on)"
[ "$seen" = "$(printf 'X=7\nY=on')" ] || fail "A: the first configure saw [$seen], want X=7 and Y=on"

if nros_grep_q '^NROS_OVERRIDE_' "$WORK/b/CMakeCache.txt"; then
    fail "B: the cache still holds an override after the configure that passed it"
fi

seen="$(configure)"
[ "$seen" = "none" ] || fail "C: a re-configure with no -D still saw [$seen] (issue 2363)"

seen="$(configure -DNROS_OVERRIDE_Y=off)"
[ "$seen" = "Y=off" ] || fail "D: a configure passing only Y saw [$seen]"

nros_grep_q '_nros_image_consume_overrides(_overrides)' "$MODULE" ||
    fail "E: nros_image_kconfig_hook does not harvest through _nros_image_consume_overrides"
if nros_grep_q '\$CACHE{NROS_OVERRIDE_' "$MODULE"; then
    fail "E: the module reads a cached override directly, which outlives its configure"
fi

if ! listing="$(python3 "$PROJECT_ROOT/scripts/build/fixtures-manifest.py" west-overrides 2>&1)"; then
    fail "F: fixtures-manifest.py west-overrides failed: $listing"
fi

if [ "$FAILURES" -ne 0 ]; then
    echo "zephyr-transient-overrides: $FAILURES failure(s)" >&2
    exit 1
fi
echo "zephyr-transient-overrides: OK (an override applies to its own configure only)"
