#!/bin/bash
# tests/cmake-cyclonedds-descriptor-cap-tests.sh -- issue 1663
#
# `NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES` on the CMAKE road. `descriptors.cpp`'s
# table drops a registration SILENTLY past its cap, the cargo road raises the
# cap from the SystemModel, and nothing raised it on the cmake roads -- where
# the model count is the wrong number anyway, because an image registers every
# descriptor whose register TU it links (issue 1663: 1 model type, 36 linked).
#
# The single writer is `nros_rmw_cyclonedds_idlc_compile`, which records each
# registry key it generates a register TU for; a deferred call sizes the table
# from the DISTINCT keys. idlc and Cyclone are not needed to test that: what
# this owns is the recording, the dedupe, the rule, and the definition reaching
# the COMPILER of the target that compiles `descriptors.cpp`. So each case is a
# real configure + build of a one-TU stand-in for `nros_rmw_cyclonedds` whose
# source `static_assert`s the value it was compiled with.
#
#   A. 300 distinct keys (each recorded twice) -> the TU compiles with 300.
#   B. 10 keys -> no definition; the TU sees the header default (it asserts
#      the macro is UNDEFINED, so a stray `-D` fails the build).
#   C. a pin above the demand wins.
#   D. a pin below the demand is a configure ERROR (negative control: the rule
#      refuses rather than compiling a table it knows is short).
#   E. a target of that name that is IMPORTED compiles nothing and is skipped.
#
# Usage: ./tests/cmake-cyclonedds-descriptor-cap-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"

MODULE="$PROJECT_ROOT/packages/rmw/cyclonedds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsDescriptorCap.cmake"

FAILURES=0
CHECKS=0
pass() { CHECKS=$((CHECKS + 1)); echo "  ok   $1"; }
fail() { CHECKS=$((CHECKS + 1)); FAILURES=$((FAILURES + 1)); echo "  FAIL $1"; }

WORK="$(mktemp -d "${TMPDIR:-/tmp}/nros-cyc-cap.XXXXXX")"
trap 'rm -rf "$WORK"' EXIT

# make_project <dir> <n-keys> <expect: number|undef> [imported]
make_project() {
    local dir="$1" n="$2" expect="$3" imported="${4:-}"
    mkdir -p "$dir/sub"
    if [ "$expect" = "undef" ]; then
        printf '%s\n' \
            '#ifdef NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES' \
            '#error "the cap was defined, and this image fits the header default"' \
            '#endif' \
            'int nros_cyc_cap_probe() { return 0; }' > "$dir/sub/d.cpp"
    else
        printf '%s\n' \
            "static_assert(NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES == $expect, \"cap\");" \
            'int nros_cyc_cap_probe() { return 0; }' > "$dir/sub/d.cpp"
    fi
    # The module is included, the target defined and the keys recorded in a
    # SUBDIRECTORY, as in a real configure (the backend and each interface
    # package are `add_subdirectory`'d): the applier runs deferred in the
    # TOP-LEVEL scope, where a value set in the including directory is not
    # visible -- the first cut's bug, which a top-level-only test passed.
    printf '%s\n' 'cmake_minimum_required(VERSION 3.22)' 'project(cyc_cap CXX)' \
        'add_subdirectory(sub)' > "$dir/CMakeLists.txt"
    {
        echo "include(\"$MODULE\")"
        if [ -n "$imported" ]; then
            echo 'add_library(nros_rmw_cyclonedds STATIC IMPORTED)'
            echo 'add_library(stand_in STATIC d.cpp)'
        else
            echo 'add_library(nros_rmw_cyclonedds STATIC d.cpp)'
        fi
        echo "foreach(_i RANGE 1 $n)"
        # Each key twice, from two "packages": the registry dedupes by NAME, so
        # the table needs the distinct count, not the TU count.
        echo '    nros_cyclonedds_record_descriptor_types("pkg::msg::dds_::T${_i}_")'
        echo '    nros_cyclonedds_record_descriptor_types("pkg::msg::dds_::T${_i}_")'
        echo 'endforeach()'
    } > "$dir/sub/CMakeLists.txt"
}

# configure_build <dir> [extra cmake args...] -> 0 on success; log in <dir>.log
configure_build() {
    local dir="$1"; shift
    cmake -S "$dir" -B "$dir/b" "$@" > "$dir.log" 2>&1 \
        && cmake --build "$dir/b" >> "$dir.log" 2>&1
}

echo "cmake-cyclonedds-descriptor-cap-tests (issue 1663)"
unset NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES

make_project "$WORK/a" 300 300
if configure_build "$WORK/a"; then
    pass "A: 300 distinct keys reach the compiler as 300"
else
    fail "A: 300 distinct keys (see below)"; tail -15 "$WORK/a.log"
fi

make_project "$WORK/b" 10 undef
if configure_build "$WORK/b" && nros_grep_q "keeps its default 256 rows (10 distinct" "$WORK/b.log"; then
    pass "B: 10 keys leave the header default (no definition), reported as 256"
else
    fail "B: 10 keys"; tail -15 "$WORK/b.log"
fi

make_project "$WORK/c" 300 400
if configure_build "$WORK/c" -DNROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES=400; then
    pass "C: a pin above the demand wins"
else
    fail "C: pin 400"; tail -15 "$WORK/c.log"
fi

make_project "$WORK/d" 300 300
if configure_build "$WORK/d" -DNROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES=100; then
    fail "D: a pin below the demand configured"
elif nros_grep_q "pinned to 100" "$WORK/d.log"; then
    pass "D: a pin below the demand is refused, naming the pin"
else
    fail "D: failed for another reason"; tail -15 "$WORK/d.log"
fi

make_project "$WORK/e" 300 undef imported
if configure_build "$WORK/e"; then
    if nros_grep_q "no target here compiles" "$WORK/e.log"; then
        pass "E: an IMPORTED backend is skipped and the skip is reported"
    else
        fail "E: built but did not report the skip"; tail -15 "$WORK/e.log"
    fi
else
    fail "E: imported"; tail -15 "$WORK/e.log"
fi

echo "$CHECKS check(s), $FAILURES failure(s)"
[ "$FAILURES" -eq 0 ]
