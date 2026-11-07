#!/bin/bash
# tests/cmake-bundled-interfaces-tests.sh -- issue 2278
#
# The CMake road refuses the bundled interfaces the way `nros configure` does.
# A message package that neither `NROS_INTERFACE_SEARCH_PATH` nor
# `AMENT_PREFIX_PATH` names, and that only the interfaces nano-ros bundles
# hold, is refused by `find_package(<pkg>)` and by `nros_generate_interfaces`,
# with the remedy configure gives. Naming the bundled root on the search path
# is the explicit opt-in, and it keeps its old place after AMENT:
#
#   1. `find_package(std_msgs)` with no ROS and no search path is refused,
#      naming the package and the remedy;
#   2. the opt-in through the environment resolves the bundled copy;
#   3. ...and through the cmake variable;
#   4. a package AMENT_PREFIX_PATH names resolves there, with no refusal;
#   5. with AMENT and the opt-in, AMENT still wins (the bundled root is not
#      promoted to the search path's first layer);
#   6. `nros_generate_interfaces(std_msgs)` auto-discovery with no ROS is
#      refused before any codegen runs;
#   7. ...and so is an explicit bundled file, the shape `resolve-deps` hands it;
#   8. the refusal helper lets a bundled file through once opted in.
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT
fail=0

# issue 1850 — this checkout's CLI, or the explicit $NROS_CLI the gate passes.
# shellcheck source=scripts/build/cargo.sh
source "$ROOT/scripts/build/cargo.sh"
NROS_BIN="${NROS_CLI:-$(nros_cli_bin --checkout-only 2>/dev/null || true)}"
if [ -z "$NROS_BIN" ]; then
    echo "  FAIL  in-tree CLI not built -- run \`just setup-cli\`"
    exit 1
fi
export NROS_CLI="$NROS_BIN"
export PATH="$(dirname "$NROS_BIN"):$PATH"

expect() {
    if [[ "$3" == *"$2"* ]]; then echo "  ok    $1"; else
        echo "  FAIL  $1 -- wanted '$2' in:"; sed 's/^/        /' <<<"$3"; fail=1; fi
}
refute() {
    if [[ "$3" != *"$2"* ]]; then echo "  ok    $1"; else
        echo "  FAIL  $1 -- did not want '$2' in:"; sed 's/^/        /' <<<"$3"; fail=1; fi
}

BUNDLED="$ROOT/packages/cli/interfaces"
REMEDY="Source ROS 2 in this shell"

# mk <dir> <body>: a project that loads the find modules the way a consumer
# does, validate-only so no codegen runs for a package that resolves.
mk() {
    local d="$WORK/$1"
    mkdir -p "$d"
    cat > "$d/CMakeLists.txt" <<CML
cmake_minimum_required(VERSION 3.22)
project(bundled_probe C CXX)
set(_NANO_ROS_PREFIX "$ROOT" CACHE INTERNAL "")
list(APPEND CMAKE_MODULE_PATH "$ROOT/cmake/find")
$2
CML
}
# run <dir> [env...]: configure with no ROS and no search path unless given.
run() {
    local d="$WORK/$1"; shift
    env -u AMENT_PREFIX_PATH -u NROS_INTERFACE_SEARCH_PATH "$@" \
        cmake -S "$d" -B "$d/b" 2>&1
}

FIND='set(NROS_FIND_PACKAGE_VALIDATE_ONLY TRUE)
find_package(std_msgs REQUIRED)
message(STATUS "probe: found=[${std_msgs_FOUND}]")'

mk find "$FIND"
rc=0; out="$(run find)" || rc=$?
[ "$rc" -ne 0 ] && echo "  ok    1. the configure fails" \
    || { echo "  FAIL  1. find_package(std_msgs) configured with no ROS and no opt-in"; fail=1; }
expect "1. the refusal names the package" '`std_msgs`' "$out"
expect "1. ...and gives configure's remedy" "$REMEDY" "$out"
expect "1. ...and names the opt-in" "NROS_INTERFACE_SEARCH_PATH=$BUNDLED" "$out"

rm -rf "$WORK/find/b"
rc=0; out="$(run find NROS_INTERFACE_SEARCH_PATH="$BUNDLED")" || rc=$?
expect "2. the opt-in (env) resolves the bundled copy" \
    "find_package(std_msgs) -> $BUNDLED/std_msgs" "$out"
refute "2. ...with no refusal" "$REMEDY" "$out"
[ "$rc" -eq 0 ] || { echo "  FAIL  2. the opted-in configure failed (rc=$rc)"; fail=1; }

mk findvar "set(NROS_INTERFACE_SEARCH_PATH \"$BUNDLED\")
$FIND"
rc=0; out="$(run findvar)" || rc=$?
expect "3. the opt-in (cmake variable) resolves the bundled copy" \
    "find_package(std_msgs) -> $BUNDLED/std_msgs" "$out"
[ "$rc" -eq 0 ] || { echo "  FAIL  3. the opted-in configure failed (rc=$rc)"; fail=1; }

ament="$WORK/ament"
mkdir -p "$ament/share/std_msgs/msg"
cp "$BUNDLED/std_msgs/package.xml" "$ament/share/std_msgs/"
cp "$BUNDLED/std_msgs/msg/String.msg" "$ament/share/std_msgs/msg/"
rm -rf "$WORK/find/b"
rc=0; out="$(run find AMENT_PREFIX_PATH="$ament")" || rc=$?
expect "4. a package AMENT names resolves there" \
    "find_package(std_msgs) -> $ament/share/std_msgs" "$out"
[ "$rc" -eq 0 ] || { echo "  FAIL  4. the AMENT configure failed (rc=$rc)"; fail=1; }

rm -rf "$WORK/find/b"
rc=0; out="$(run find AMENT_PREFIX_PATH="$ament" NROS_INTERFACE_SEARCH_PATH="$BUNDLED")" || rc=$?
expect "5. with AMENT and the opt-in, AMENT still wins" \
    "find_package(std_msgs) -> $ament/share/std_msgs" "$out"

GEN="include(\"$ROOT/cmake/NanoRosGenerateInterfaces.cmake\")"
mk gen "$GEN
nros_generate_interfaces(std_msgs LANGUAGE C SKIP_INSTALL)"
rc=0; out="$(run gen)" || rc=$?
[ "$rc" -ne 0 ] && echo "  ok    6. auto-discovery fails the configure" \
    || { echo "  FAIL  6. nros_generate_interfaces(std_msgs) generated from the bundled set"; fail=1; }
expect "6. ...naming the package and the remedy" '`std_msgs`' "$out"
expect "6. ...with configure's remedy" "$REMEDY" "$out"

mk genfile "$GEN
nros_generate_interfaces(std_msgs \"$BUNDLED/std_msgs/msg/String.msg\" LANGUAGE C SKIP_INSTALL)"
rc=0; out="$(run genfile)" || rc=$?
[ "$rc" -ne 0 ] && echo "  ok    7. an explicit bundled file fails the configure" \
    || { echo "  FAIL  7. an explicit bundled file was generated"; fail=1; }
expect "7. ...with configure's remedy" "$REMEDY" "$out"

mk helper "$GEN
_nros_refuse_bundled_interfaces(std_msgs \"$ROOT\" \"$BUNDLED/std_msgs/msg/String.msg\")
message(STATUS \"probe: helper passed\")"
rc=0; out="$(run helper NROS_INTERFACE_SEARCH_PATH="$BUNDLED")" || rc=$?
expect "8. the opted-in file passes the refusal" "probe: helper passed" "$out"

if [ "$fail" -ne 0 ]; then
    echo "cmake-bundled-interfaces: FAILED"
    exit 1
fi
echo "cmake-bundled-interfaces: OK"
