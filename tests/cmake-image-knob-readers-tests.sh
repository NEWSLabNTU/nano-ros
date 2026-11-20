#!/bin/bash
# tests/cmake-image-knob-readers-tests.sh -- issue 2496
#
# A CONFIGURE-TIME CMAKE READER OF A KNOB SEES THE IMAGE'S `[image.<id>.knobs]`.
#
# On the cmake road the image's knob rows rode only the cargo commands the
# configure spawns (`NanoRosImageEnv.cmake`, issue 1712). A reader that runs
# in CMake itself never saw them: `[image.<id>.knobs.memory] app_stack_bytes =
# 20480` left a FreeRTOS C image's `.app_stack_bytes` at 65536. Such a reader now
# asks `nros_image_env_value`, the one answer on both roads (the cmake road's
# image-env file, the west road's recorded `NROS_IMAGE_ENV_ROW_<name>`).
#
# WHAT IS ASSERTED (script mode, no toolchain)
#
#   A. the image's row reaches `_nros_freertos_entry_app_stack`: 20480 bytes.
#   B. a `-D` still wins over the row.
#   C. the environment still wins over the row (cargo's own precedence).
#   D. no row anywhere keeps the 64 KiB default.
#   E. the WEST road's recorded row (no image-env file) is read too.
#   F. a row that is not a positive whole number is refused, naming the
#      system.toml spelling.
#   G. the Cyclone descriptor cap's module provides the same reader, on a
#      configure that included nothing else.
#
# Usage: ./tests/cmake-image-knob-readers-tests.sh
# Exit:  0 all assertions held; 1 otherwise.

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# shellcheck source=lib/common.sh
source "$SCRIPT_DIR/lib/common.sh"
# shellcheck source=../scripts/lib/grep-q.sh
source "$PROJECT_ROOT/scripts/lib/grep-q.sh"

FAILURES=0
fail() {
    log_error "$*"
    FAILURES=$((FAILURES + 1))
}

if ! command -v cmake >/dev/null 2>&1; then
    fail "cmake is not on PATH -- this test cannot report a verdict without it"
    exit 1
fi

init_test_tmpdir "nros-image-knob-readers"
trap 'cleanup_test_tmpdir' EXIT
unset NROS_MEMORY_APP_STACK_BYTES NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES

ROW="$TEST_TMPDIR/image-env.toml"
printf '%s\n' 'NROS_MEMORY_APP_STACK_BYTES = "20480"' > "$ROW"
BAD="$TEST_TMPDIR/bad-env.toml"
printf '%s\n' 'NROS_MEMORY_APP_STACK_BYTES = "abc"' > "$BAD"

printf '%s\n' \
    'cmake_minimum_required(VERSION 3.22)' \
    'include("${NROS_REPO_DIR}/cmake/NanoRosBoardNet.cmake")' \
    'if(ENVTOML)' \
    '  set_property(GLOBAL PROPERTY NROS_IMAGE_ENV_CONFIG "${ENVTOML}")' \
    'endif()' \
    'if(WESTROW)' \
    '  set(NROS_IMAGE_ENV_ROWS NROS_MEMORY_APP_STACK_BYTES CACHE INTERNAL "")' \
    '  set(NROS_IMAGE_ENV_ROW_NROS_MEMORY_APP_STACK_BYTES "${WESTROW}" CACHE INTERNAL "")' \
    'endif()' \
    '_nros_freertos_entry_app_stack(_b)' \
    'message(STATUS "BYTES=${_b}")' > "$TEST_TMPDIR/stack.cmake"

# stack <log> [cmake -D args...] -> sets RC
stack() {
    local log="$1"
    shift
    RC=0
    (cd "$TEST_TMPDIR" && cmake -DNROS_REPO_DIR="$PROJECT_ROOT" "$@" \
        -P "$TEST_TMPDIR/stack.cmake") >"$log" 2>&1 || RC=$?
}

expect_bytes() {  # <case> <log> <bytes>
    if [ "$RC" -ne 0 ] || ! nros_grep_q "BYTES=$3\$" "$2"; then
        fail "$1: expected BYTES=$3 (rc=$RC)"
        tail -8 "$2" >&2
    fi
}

stack "$TEST_TMPDIR/a.log" -DENVTOML="$ROW"
expect_bytes "A (the image's row reaches the app stack)" "$TEST_TMPDIR/a.log" 20480

stack "$TEST_TMPDIR/b.log" -DENVTOML="$ROW" -DNROS_MEMORY_APP_STACK_BYTES=32768
expect_bytes "B (a -D wins over the row)" "$TEST_TMPDIR/b.log" 32768

RC=0
(cd "$TEST_TMPDIR" && NROS_MEMORY_APP_STACK_BYTES=24576 cmake -DNROS_REPO_DIR="$PROJECT_ROOT" \
    -DENVTOML="$ROW" -P "$TEST_TMPDIR/stack.cmake") >"$TEST_TMPDIR/c.log" 2>&1 || RC=$?
expect_bytes "C (the environment wins over the row)" "$TEST_TMPDIR/c.log" 24576

stack "$TEST_TMPDIR/d.log"
expect_bytes "D (no row keeps the default)" "$TEST_TMPDIR/d.log" 65536

stack "$TEST_TMPDIR/e.log" -DWESTROW=28672
expect_bytes "E (the west road's recorded row)" "$TEST_TMPDIR/e.log" 28672

stack "$TEST_TMPDIR/f.log" -DENVTOML="$BAD"
if [ "$RC" -eq 0 ] || ! nros_grep_q 'app_stack_bytes' "$TEST_TMPDIR/f.log"; then
    fail "F: a non-number row was not refused naming app_stack_bytes (rc=$RC)"
    tail -8 "$TEST_TMPDIR/f.log" >&2
fi

printf '%s\n' \
    'cmake_minimum_required(VERSION 3.22)' \
    'include("${NROS_REPO_DIR}/packages/rmw/cyclonedds/nros-rmw-cyclonedds/cmake/NrosRmwCycloneddsDescriptorCap.cmake")' \
    'set(NROS_IMAGE_ENV_ROWS NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES CACHE INTERNAL "")' \
    'set(NROS_IMAGE_ENV_ROW_NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES 512 CACHE INTERNAL "")' \
    'if(NOT COMMAND nros_image_env_value)' \
    '  message(FATAL_ERROR "the Cyclone cap module does not provide nros_image_env_value")' \
    'endif()' \
    'nros_image_env_value(_pin NROS_CYCLONEDDS_MAX_DESCRIPTOR_TYPES)' \
    'message(STATUS "PIN=${_pin}")' > "$TEST_TMPDIR/cyclone.cmake"
RC=0
(cd "$TEST_TMPDIR" && cmake -DNROS_REPO_DIR="$PROJECT_ROOT" -P "$TEST_TMPDIR/cyclone.cmake") \
    >"$TEST_TMPDIR/g.log" 2>&1 || RC=$?
if [ "$RC" -ne 0 ] || ! nros_grep_q 'PIN=512$' "$TEST_TMPDIR/g.log"; then
    fail "G: the Cyclone cap module does not read the image's row (rc=$RC)"
    tail -8 "$TEST_TMPDIR/g.log" >&2
fi

if [ "$FAILURES" -ne 0 ]; then
    log_error "cmake-image-knob-readers: $FAILURES failure(s)"
    exit 1
fi
log_success "cmake-image-knob-readers: A-G (issue 2496)"
exit 0
