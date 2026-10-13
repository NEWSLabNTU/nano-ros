#!/bin/bash
# tests/cmake-retired-names-tests.sh -- phase-484 W8
#
# `cmake/NanoRosRetiredNames.cmake` refuses every `NANO_ROS_*` name in its
# table, naming the `NROS_*` replacement. Asserted, in a real `project()` (the
# macro reads the caller's directory scope):
#
#   1. a `-DNANO_ROS_PLATFORM=…` CACHE entry fails the configure, naming
#      `NROS_PLATFORM`;
#   2. a normal `set(NANO_ROS_FEATURES …)` fails the same way;
#   3. old and new set together (even AGREEING) still fails — the old name set
#      at all is the error;
#   4. the new name alone configures and prints nothing about retired names;
#   5. the W7b-2 twins (`NANO_ROS_ROOT`, `NANO_ROS_RMW`) fail naming
#      `NROS_REPO_DIR` / `NROS_RMW`, both reported in one configure;
#   6. the retired `NANO_ROS_ROOT <path>` KEYWORD fails naming `NROS_REPO_DIR`,
#      and the `NROS_REPO_DIR` keyword alone is accepted;
#   7. a build dir whose cache carries old-name RESIDUE (case 1's dir) is
#      repaired by `cmake-cache-guard.sh`'s retired-name rule and then
#      configures with the new name.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0

mk() {
    mkdir -p "$WORK/$1"
    cat > "$WORK/$1/CMakeLists.txt" <<CML
cmake_minimum_required(VERSION 3.22)
project(rn NONE)
$2
include("$ROOT/cmake/NanoRosRetiredNames.cmake")
nros_refuse_retired_names()
message(STATUS "NROS_PLATFORM=[\${NROS_PLATFORM}]")
CML
}

run() { cmake -S "$WORK/$1" -B "$WORK/$1/b" "${@:2}" 2>&1; }

# expect_fail <case> <label> <needle>... -- cmake args after `--`
expect_fail() {
    local case="$1" label="$2"; shift 2
    local needles=()
    while [ "$1" != "--" ]; do needles+=("$1"); shift; done
    shift
    local out rc=0
    out="$(run "$case" "$@")" || rc=$?
    if [ "$rc" -eq 0 ]; then
        echo "  FAIL $label: configured"; echo "$out" | sed 's/^/       /'; fail=1; return
    fi
    for n in "${needles[@]}"; do
        if [[ "$out" != *"$n"* ]]; then
            echo "  FAIL $label: error does not say '$n'"; echo "$out" | sed 's/^/       /'; fail=1; return
        fi
    done
    echo "  ok   $label"
}

mk cache ""
expect_fail cache "-DNANO_ROS_PLATFORM is a configure error naming NROS_PLATFORM" \
    "NANO_ROS_PLATFORM=\"nuttx\"" "set NROS_PLATFORM instead" "cmake -U NANO_ROS_PLATFORM" -- -DNANO_ROS_PLATFORM=nuttx

mk normal 'set(NANO_ROS_FEATURES "safety")'
expect_fail normal "set(NANO_ROS_FEATURES) is a configure error naming NROS_FEATURES" \
    "set NROS_FEATURES instead" --

mk agree ""
expect_fail agree "old and new set to the SAME value still fail" \
    "set NROS_PLATFORM instead" -- -DNANO_ROS_PLATFORM=posix -DNROS_PLATFORM=posix

mk new ""
rc=0; out="$(run new -DNROS_PLATFORM=posix)" || rc=$?
if [ "$rc" -eq 0 ] && [[ "$out" == *"NROS_PLATFORM=[posix]"* && "$out" != *"retired"* && "$out" != *"deprecat"* ]]; then
    echo "  ok   the new name alone configures silently"
else echo "  FAIL new name:"; echo "$out" | sed 's/^/       /'; fail=1; fi

mk root ""
expect_fail root "-DNANO_ROS_ROOT / -DNANO_ROS_RMW fail naming NROS_REPO_DIR / NROS_RMW" \
    "set NROS_REPO_DIR instead" "set NROS_RMW instead" -- -DNANO_ROS_ROOT=/opt/nano-ros -DNANO_ROS_RMW=xrce

kw_fn='function(fake_verb)
    cmake_parse_arguments(_FV "" "NROS_REPO_DIR;NANO_ROS_ROOT" "" ${ARGN})
    nros_refuse_retired_root_keyword(_FV fake_verb)
    message(STATUS "repo=[${_FV_NROS_REPO_DIR}]")
endfunction()'
mkdir -p "$WORK/kw" "$WORK/kwnew"
printf '%s\n' "cmake_minimum_required(VERSION 3.22)" "project(rn NONE)" \
    "include(\"$ROOT/cmake/NanoRosRetiredNames.cmake\")" "$kw_fn" \
    "fake_verb(NANO_ROS_ROOT /opt/nano-ros)" > "$WORK/kw/CMakeLists.txt"
expect_fail kw "the NANO_ROS_ROOT keyword is a configure error naming NROS_REPO_DIR" \
    "fake_verb: the NANO_ROS_ROOT keyword is retired" "NROS_REPO_DIR <path>" --
printf '%s\n' "cmake_minimum_required(VERSION 3.22)" "project(rn NONE)" \
    "include(\"$ROOT/cmake/NanoRosRetiredNames.cmake\")" "$kw_fn" \
    "fake_verb(NROS_REPO_DIR /opt/nano-ros)" > "$WORK/kwnew/CMakeLists.txt"
rc=0; out="$(run kwnew)" || rc=$?
if [ "$rc" -eq 0 ] && [[ "$out" == *"repo=[/opt/nano-ros]"* ]]; then
    echo "  ok   the NROS_REPO_DIR keyword alone is accepted"
else echo "  FAIL NROS_REPO_DIR keyword:"; echo "$out" | sed 's/^/       /'; fail=1; fi

rc=0; out="$(bash -c '. "$1/scripts/build/cmake-cache-guard.sh"; nros_cmake_guard_retired_names "$2"' _ "$ROOT" "$WORK/cache/b" 2>&1 \
    && run cache -DNROS_PLATFORM=posix)" || rc=$?
if [ "$rc" -eq 0 ] && [[ "$out" == *"dropping those entries"* && "$out" == *"NROS_PLATFORM=[posix]"* ]] \
    && ! grep -q '^NANO_ROS_PLATFORM:' "$WORK/cache/b/CMakeCache.txt"; then
    echo "  ok   the cache guard drops old-name residue and the dir reconfigures"
else echo "  FAIL residue repair:"; echo "$out" | sed 's/^/       /'; fail=1; fi

if [ "$fail" -ne 0 ]; then echo "cmake-retired-names: FAILED"; exit 1; fi
echo "cmake-retired-names: OK (8 cases)"
