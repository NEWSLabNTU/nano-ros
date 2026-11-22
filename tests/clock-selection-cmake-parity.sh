#!/usr/bin/env bash
# The CMake clock-selection reader (cmake/NanoRosClockSelection.cmake) answers
# the same table as the Rust one (nros_zephyr_build::clock_selection's
# `the_shared_case_table` test): tests/data/clock-selection-cases.tsv. Each row
# configures a scratch project, with the knobs exported (the top rung
# `nros_image_env_value` reads), and compares the target's compile definitions.
set -euo pipefail
root="$(cd "$(dirname "$0")/.." && pwd)"
table="$root/tests/data/clock-selection-cases.tsv"
api="$root/packages/platform/nros-platform-api/include"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$tmp/src"
cat >"$tmp/src/CMakeLists.txt" <<CML
cmake_minimum_required(VERSION 3.22)
project(clock_selection_parity C)
include("$root/cmake/NanoRosClockSelection.cmake")
file(WRITE "\${CMAKE_BINARY_DIR}/empty.c" "int x;\n")
add_library(probe STATIC "\${CMAKE_BINARY_DIR}/empty.c")
nros_clock_selection_apply(probe "$api")
get_target_property(_d probe COMPILE_DEFINITIONS)
if(NOT _d)
    set(_d NONE)
endif()
file(WRITE "\${CMAKE_BINARY_DIR}/defines.txt" "\${_d}")
CML
pass=0 fail=0 n=0
while IFS=$'\t' read -r src up fail_p settle want; do
    case "$src" in '#'*|'') continue ;; esac
    n=$((n + 1))
    envs=(-u NROS_CLOCK_SOURCE -u NROS_CLOCK_UPGRADE -u NROS_CLOCK_EPOCH_FAILURE -u NROS_CLOCK_FALLBACK_SETTLE_MS)
    envs+=("NROS_CLOCK_SOURCE=$src")
    [ "$up" = "-" ] || envs+=("NROS_CLOCK_UPGRADE=$up")
    [ "$fail_p" = "-" ] || envs+=("NROS_CLOCK_EPOCH_FAILURE=$fail_p")
    [ "$settle" = "-" ] || envs+=("NROS_CLOCK_FALLBACK_SETTLE_MS=$settle")
    b="$tmp/b$n"
    if ! env "${envs[@]}" cmake -S "$tmp/src" -B "$b" >"$b.log" 2>&1; then
        echo "  FAIL row $n ($src): configure failed"; sed 's/^/    /' "$b.log" | tail -5
        fail=$((fail + 1)); continue
    fi
    got="$(cat "$b/defines.txt")"
    if [ "$got" = "$want" ]; then
        pass=$((pass + 1)); echo "  ok   row $n ($src)"
    else
        fail=$((fail + 1)); echo "  FAIL row $n ($src)"; echo "    want: $want"; echo "    got:  $got"
    fi
done <"$table"
echo "clock-selection-cmake-parity: $pass passed, $fail failed (of $n rows)"
[ "$fail" -eq 0 ] && [ "$n" -ge 5 ]
