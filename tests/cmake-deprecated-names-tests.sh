#!/bin/bash
# tests/cmake-deprecated-names-tests.sh -- phase-484 W7b
#
# `cmake/NanoRosDeprecatedNames.cmake` keeps every `NANO_ROS_*` name in its
# table working as a DEPRECATED ALIAS of its `NROS_*` name. Asserted, in a real
# `project()` (the macro sets variables in the caller's directory scope):
#
#   1. a `-DNANO_ROS_PLATFORM=…` CACHE entry is adopted as `NROS_PLATFORM`,
#      and the configure prints a DEPRECATION naming the new name;
#   2. a normal `set(NANO_ROS_FEATURES …)` is adopted the same way;
#   3. the old and new names set to DIFFERENT values is a FATAL_ERROR;
#   4. the new name alone is untouched and prints no deprecation.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0

mk() {
    mkdir -p "$WORK/$1"
    cat > "$WORK/$1/CMakeLists.txt" <<CML
cmake_minimum_required(VERSION 3.22)
project(dn NONE)
$2
include("$ROOT/cmake/NanoRosDeprecatedNames.cmake")
nros_adopt_deprecated_names()
message(STATUS "NROS_PLATFORM=[\${NROS_PLATFORM}] NROS_FEATURES=[\${NROS_FEATURES}]")
CML
}

run() { cmake -S "$WORK/$1" -B "$WORK/$1/b" "${@:2}" 2>&1; }

mk cache ""
out="$(run cache -DNANO_ROS_PLATFORM=nuttx)"
if [[ "$out" == *"NROS_PLATFORM=[nuttx]"* && "$out" == *"NANO_ROS_PLATFORM is deprecated; use NROS_PLATFORM"* ]]; then
    echo "  ok   -DNANO_ROS_PLATFORM is adopted as NROS_PLATFORM, with a deprecation"
else echo "  FAIL cache alias:"; echo "$out" | sed 's/^/       /'; fail=1; fi

mk normal 'set(NANO_ROS_FEATURES "safety")'
out="$(run normal)"
if [[ "$out" == *"NROS_FEATURES=[safety]"* ]]; then
    echo "  ok   set(NANO_ROS_FEATURES) is adopted as NROS_FEATURES"
else echo "  FAIL normal alias:"; echo "$out" | sed 's/^/       /'; fail=1; fi

mk clash ""
if out="$(run clash -DNANO_ROS_PLATFORM=nuttx -DNROS_PLATFORM=posix)"; then
    echo "  FAIL disagreeing old and new names configured:"; echo "$out" | sed 's/^/       /'; fail=1
elif [[ "$out" == *"disagree"* ]]; then
    echo "  ok   NANO_ROS_PLATFORM != NROS_PLATFORM is a configure error"
else echo "  FAIL clash error text:"; echo "$out" | sed 's/^/       /'; fail=1; fi

mk new ""
out="$(run new -DNROS_PLATFORM=posix)"
if [[ "$out" == *"NROS_PLATFORM=[posix]"* && "$out" != *"deprecated"* ]]; then
    echo "  ok   the new name alone prints no deprecation"
else echo "  FAIL new name:"; echo "$out" | sed 's/^/       /'; fail=1; fi

if [ "$fail" -ne 0 ]; then echo "cmake-deprecated-names: FAILED"; exit 1; fi
echo "cmake-deprecated-names: OK (4 cases)"
