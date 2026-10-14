#!/bin/bash
# tests/cmake-entry-agreement-tests.sh -- issue 1894
#
# `cmake/NanoRosEntryAgreement.cmake`: an entry's DEPLOY/BOARD keyword against
# the leaf `system.toml` that `find_package(nano_ros)` read into NROS_EXPORT_*.
# Asserted in a real `project()`, with NROS_EXPORT_* set the way
# `nano_ros_read_leaf_system()` sets them:
#
#   1. no system.toml read -> any keyword stands (a guest project);
#   2. a keyword that agrees is accepted;
#   3. a DEPLOY that differs is a configure error naming both places;
#   4. a BOARD that differs is one too;
#   5. a board may be named by its alias, its cmake module, or -- when it IS
#      its deploy token, stored as "" -- by that token.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0

# case <name> <found> <deploy> <board> <cmake_board> <call args…>
case_() {
    local name="$1" found="$2" deploy="$3" board="$4" cmake_board="$5"
    shift 5
    mkdir -p "$WORK/$name"
    cat > "$WORK/$name/CMakeLists.txt" <<CML
cmake_minimum_required(VERSION 3.22)
project(ea NONE)
include("$ROOT/cmake/NanoRosEntryAgreement.cmake")
set(NROS_EXPORT_FOUND $found)
set(NROS_EXPORT_SYSTEM_TOML "/leaf/system.toml")
set(NROS_EXPORT_IMAGE "img")
set(NROS_EXPORT_DEPLOY "$deploy")
set(NROS_EXPORT_BOARD "$board")
set(NROS_EXPORT_CMAKE_BOARD "$cmake_board")
nros_entry_keywords_agree("nano_ros_add_executable(app)" $*)
message(STATUS "AGREED")
CML
    cmake -S "$WORK/$name" -B "$WORK/$name/b" 2>&1 || true
}
expect() {
    if [[ "$3" == *"$2"* ]]; then echo "  ok    $1"; else
        echo "  FAIL  $1 -- wanted '$2' in:"; sed 's/^/        /' <<<"$3"; fail=1; fi
}

expect "1. no system.toml: DEPLOY stands" "AGREED" \
    "$(case_ guest FALSE "" "" "" DEPLOY zephyr BOARD qemu-arm)"
expect "2. agreeing DEPLOY and BOARD" "AGREED" \
    "$(case_ agree TRUE freertos mps2-an385 mps2-an385 DEPLOY freertos BOARD mps2-an385)"
out="$(case_ deploy TRUE linux "" "" DEPLOY zephyr)"
expect "3. differing DEPLOY is refused" '`DEPLOY zephyr` disagrees with' "$out"
expect "3. and names the image's deploy" 'deploys to `linux`' "$out"
out="$(case_ board TRUE freertos mps2-an385 mps2-an385 BOARD stm32f4)"
expect "4. differing BOARD is refused" '`BOARD stm32f4` disagrees with' "$out"
expect "4. and names the image's board" 'names board `mps2-an385`' "$out"
expect "5. board by its cmake module" "AGREED" \
    "$(case_ module TRUE freertos my-alias mps2-an385 BOARD mps2-an385)"
expect "5. board that is its deploy token" "AGREED" \
    "$(case_ token TRUE linux "" "" BOARD linux)"
expect "5. but not another token" '`BOARD zephyr` disagrees' \
    "$(case_ token2 TRUE linux "" "" BOARD zephyr)"

[ "$fail" -eq 0 ] && echo "cmake-entry-agreement: OK" || { echo "cmake-entry-agreement: FAILED"; exit 1; }
