#!/bin/bash
# tests/cmake-leaf-cache-tests.sh -- issue 0934 R1
#
# `cmake/NanoRosLeafCache.cmake` caches a leaf `system.toml` fact and keeps it
# in step with the file. Asserted in a real `project()`, across RECONFIGURES of
# one build dir (the shape that went stale):
#
#   1. the first configure caches the leaf's value;
#   2. the leaf's value changes -> the reconfigure FOLLOWS it (was: stuck);
#   3. a `-D` a person passed wins over the leaf, and keeps winning on the
#      next reconfigure;
#   4. a normal `set()` in the project before the include wins, and the cache
#      is left untouched.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0

mk() {
    mkdir -p "$WORK/$1"
    cat > "$WORK/$1/CMakeLists.txt" <<CML
cmake_minimum_required(VERSION 3.22)
project(lc NONE)
$3
include("$ROOT/cmake/NanoRosLeafCache.cmake")
nros_leaf_cache_set(NROS_RMW "$2" "RMW backend (from the leaf system.toml rmw)")
message(STATUS "NROS_RMW=[\${NROS_RMW}] cache=[\$CACHE{NROS_RMW}]")
CML
}
run() { cmake -S "$WORK/$1" -B "$WORK/$1/b" "${@:2}" 2>&1; }
expect() {
    if [[ "$3" == *"$2"* ]]; then echo "  ok    $1"; else
        echo "  FAIL  $1 -- wanted '$2' in:"; sed 's/^/        /' <<<"$3"; fail=1; fi
}

mk follow zenoh ""
expect "1. first configure caches the leaf value" "NROS_RMW=[zenoh]" "$(run follow)"
mk follow xrce ""
out="$(run follow)"
expect "2. a changed leaf value is followed" "NROS_RMW=[xrce]" "$out"
expect "2. and the move is reported" '"zenoh" -> "xrce"' "$out"

mk dash zenoh ""
expect "3. a -D wins" "NROS_RMW=[cyclonedds]" "$(run dash -DNROS_RMW=cyclonedds)"
mk dash xrce ""
expect "3. and keeps winning on reconfigure" "NROS_RMW=[cyclonedds]" "$(run dash)"

mk normal zenoh 'set(NROS_RMW xrce)'
expect "4. a project's own set() wins" "NROS_RMW=[xrce] cache=[]" "$(run normal)"

[ "$fail" -eq 0 ] && echo "cmake-leaf-cache: OK" || { echo "cmake-leaf-cache: FAILED"; exit 1; }
