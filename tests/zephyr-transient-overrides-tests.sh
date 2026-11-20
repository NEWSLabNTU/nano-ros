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
#   G  every retired Kconfig symbol and -DNROS_OVERRIDE_<ID> is refused
#      naming its replacement and, for a unit change, the factor (phase-581 W5)
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

# G  phase-581 W5 (RFC-0140 D4) -- EVERY retired Kconfig symbol (a conf line
#    and a -DCONFIG_ cache entry) and EVERY retired -DNROS_OVERRIDE_<ID> is
#    refused naming its replacement, and a unit change names its factor. The
#    cases are generated from the registry's own lists, so a new retirement is
#    covered the day it lands.
mkdir -p "$WORK/g"
printf '%s\n' \
    'cmake_minimum_required(VERSION 3.22)' \
    'project(probe NONE)' \
    'include("${MODULE}")' \
    'set(_conf "${CMAKE_BINARY_DIR}/retired.conf")' \
    'set(_lines "")' \
    'foreach(_s IN LISTS NROS_KNOB_RETIRED_KCONFIG)' \
    '    string(APPEND _lines "CONFIG_${_s}=1\n")' \
    'endforeach()' \
    'file(WRITE "${_conf}" "${_lines}")' \
    'set(_ids "")' \
    'foreach(_o IN LISTS NROS_KNOB_RETIRED_OVERRIDES)' \
    '    string(REGEX REPLACE "^NROS_OVERRIDE_" "" _id "${_o}")' \
    '    list(APPEND _ids "${_id}")' \
    'endforeach()' \
    '_nros_image_kconfig_retired_findings(_found "${_ids}" "${_conf}")' \
    'list(LENGTH NROS_KNOB_RETIRED_KCONFIG _nk)' \
    'list(LENGTH NROS_KNOB_RETIRED_OVERRIDES _no)' \
    'message(STATUS "PROBE_COUNTS ${_nk} ${_no}")' \
    'string(REPLACE "\n" ";" _found "${_found}")' \
    'foreach(_l IN LISTS _found)' \
    '    if(NOT _l STREQUAL "")' \
    '        message(STATUS "PROBE_RETIRED ${_l}")' \
    '    endif()' \
    'endforeach()' \
    > "$WORK/g/CMakeLists.txt"
gout="$(cmake -S "$WORK/g" -B "$WORK/gb" "-DMODULE=$MODULE" 2>&1)"
read -r nk no <<< "$(printf '%s\n' "$gout" | sed -n 's/.*PROBE_COUNTS //p')"
nk_seen="$(printf '%s\n' "$gout" | grep -c 'PROBE_RETIRED .*retired.conf: CONFIG_.* is retired: set CONFIG_NROS_' || true)"
no_seen="$(printf '%s\n' "$gout" | grep -c 'PROBE_RETIRED *-DNROS_OVERRIDE_.* is retired: pass -DNROS_OVERRIDE_' || true)"
if [ -z "$nk" ] || [ "$nk" -lt 1 ] || [ "$nk_seen" != "$nk" ]; then
    fail "G: $nk_seen of ${nk:-?} retired Kconfig symbols were refused naming their replacement"
fi
if [ -z "$no" ] || [ "$no" -lt 1 ] || [ "$no_seen" != "$no" ]; then
    fail "G: $no_seen of ${no:-?} retired overrides were refused naming their replacement"
fi
printf '%s\n' "$gout" > "$WORK/g.log"
nros_grep_q 'CONFIG_NROS_EXECUTOR_BACKING_U64S is retired: .*(the value x 8' "$WORK/g.log" ||
    fail "G: a unit change (backing_u64s, x 8) does not name its factor"

if [ "$FAILURES" -ne 0 ]; then
    echo "zephyr-transient-overrides: $FAILURES failure(s)" >&2
    exit 1
fi
echo "zephyr-transient-overrides: OK (an override applies to its own configure only)"
