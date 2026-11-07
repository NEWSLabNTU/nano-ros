#!/usr/bin/env bash
# A build dir whose resolved compiler changed is reconfigured, not refused
# (phase-548 W6).
#
# After `nros setup` installs the pinned cross GCC, the next `nros build` used
# to stop at NanoRosCrossToolchain.cmake's refusal, "this build directory is
# already configured with a DIFFERENT C compiler", and the user deleted the
# build directory by hand. The toolchain file now also writes
# `<build>/.nros-toolchain-changed`, and `nros build` answers it by removing
# that directory's CMakeCache.txt and CMakeFiles/ (what `cmake --fresh` does)
# and configuring again (cmd/build.rs, `reconfigure_after_toolchain_change`,
# unit-tested there).
#
# This drives the real module through real configures, with two stand-in
# "cross compilers" (wrappers around the host cc, at different paths, so their
# real paths differ the way an SDK-store GCC and a PATH GCC do):
#
#   1. configure with prefix A: succeeds, no marker;
#   2. switch to prefix B, configure the same dir: refused, marker written,
#      naming both compilers;
#   3. the fresh step `nros build` takes, then configure with B: succeeds,
#      the marker is gone, and the build tree's compiler is B.
#
# Buildless apart from cmake's own compiler probe (the toolchain declares the
# compiler working, as the real ones do). Needs cmake and a host cc.
set -euo pipefail
cd "$(dirname "$0")/../.."
root="$PWD"
# shellcheck source=scripts/lib/grep-q.sh
. scripts/lib/grep-q.sh

missing=""
for t in cmake cc c++ ar ranlib; do command -v "$t" >/dev/null 2>&1 || missing="$missing $t"; done
if [ -n "$missing" ]; then
    # shellcheck source=scripts/build/check-skip.sh
    . scripts/build/check-skip.sh
    nros_check_skip toolchain-change-reconfigure "missing host tool(s):$missing"
    exit 0
fi

tmp="$(mktemp -d "${TMPDIR:-/tmp}/nros-toolchain-change.XXXXXX")"
trap 'rm -r "$tmp"' EXIT

for side in a b; do
    mkdir -p "$tmp/$side"
    for pair in gcc:cc g++:c++ ar:ar ranlib:ranlib; do
        tool="${pair%%:*}" host="${pair##*:}"
        printf '#!/bin/sh\nexec %s "$@"\n' "$(command -v "$host")" > "$tmp/$side/fake-$tool"
        chmod 755 "$tmp/$side/fake-$tool"
    done
done

# Self-test, on the normal path: the premise. The toolchain module compares
# REAL paths, so two stand-ins that resolved to one file would make step 2 a
# configure with the same compiler, and the check would prove nothing.
selftest() {
    local ra rb
    ra="$(realpath "$tmp/a/fake-gcc")" rb="$(realpath "$tmp/b/fake-gcc")"
    if [ "$ra" = "$rb" ]; then
        echo "check-toolchain-change-reconfigure SELFTEST FAILED: both stand-in compilers are $ra" >&2
        return 1
    fi
}
selftest || exit 2

cat > "$tmp/toolchain.cmake" <<EOF
set(CMAKE_SYSTEM_NAME Generic)
include("$root/cmake/toolchain/NanoRosCrossToolchain.cmake")
nros_cross_toolchain_resolve(
    TOOL         arm-none-eabi-gcc
    PREFIXES     arm-none-eabi
    OVERRIDE_VAR NROS_TEST_FAKE_PREFIX
    OUT_PREFIX   _P
    OUT_ORIGIN   _O)
nros_cross_toolchain_report(
    TOOL   arm-none-eabi-gcc
    PREFIX "\${_P}"
    ORIGIN "\${_O}"
    OVERRIDE_VAR NROS_TEST_FAKE_PREFIX)
set(CMAKE_C_COMPILER   \${_P}-gcc)
set(CMAKE_CXX_COMPILER \${_P}-g++)
set(CMAKE_C_COMPILER_WORKS   TRUE CACHE BOOL "" FORCE)
set(CMAKE_CXX_COMPILER_WORKS TRUE CACHE BOOL "" FORCE)
set(CMAKE_TRY_COMPILE_TARGET_TYPE STATIC_LIBRARY)
EOF
mkdir -p "$tmp/src"
printf 'cmake_minimum_required(VERSION 3.20)\nproject(t C CXX)\n' > "$tmp/src/CMakeLists.txt"

fail=0
configure() {   # <prefix-dir>
    local tc="-DCMAKE_TOOLCHAIN_FILE=$tmp/toolchain.cmake"
    NROS_TEST_FAKE_PREFIX="$1/fake" cmake -S "$tmp/src" -B "$tmp/build" "$tc" > "$tmp/log" 2>&1  # copy-out-root-ok: a non-nano-ros project that includes only the toolchain module
}
marker="$tmp/build/.nros-toolchain-changed"

if ! configure "$tmp/a"; then
    echo "FAIL: the first configure (prefix A) failed:" >&2; sed 's/^/  /' "$tmp/log" >&2; exit 1
fi
[ ! -e "$marker" ] || { echo "FAIL: a clean configure left a marker" >&2; fail=1; }

if configure "$tmp/b"; then
    echo "FAIL: a configure with a DIFFERENT compiler succeeded in place; it must refuse" >&2
    fail=1
elif [ ! -f "$marker" ]; then
    echo "FAIL: the refusal wrote no $marker, so nros build cannot tell it apart" >&2
    sed 's/^/  /' "$tmp/log" >&2
    fail=1
elif ! nros_grep_q -F "have=$tmp/a/fake-gcc" "$marker" || ! nros_grep_q -F "want=$tmp/b/fake-gcc" "$marker"; then
    echo "FAIL: the marker does not name both compilers:" >&2; sed 's/^/  /' "$marker" >&2
    fail=1
fi

# What `nros build` does on the marker: cmake --fresh's two removals, then
# configure again.
rm -f "$tmp/build/CMakeCache.txt" "$marker"
rm -r "$tmp/build/CMakeFiles"
if ! configure "$tmp/b"; then
    echo "FAIL: the reconfigure after the fresh step failed:" >&2; sed 's/^/  /' "$tmp/log" >&2
    fail=1
elif [ -e "$marker" ]; then
    echo "FAIL: a successful reconfigure left the marker" >&2; fail=1
elif ! nros_grep_q -F "$tmp/b/fake-gcc" <<<"$(cat "$tmp/build/CMakeFiles/"*/CMakeCCompiler.cmake 2>/dev/null)"; then
    echo "FAIL: after the reconfigure the build tree does not use prefix B's compiler" >&2; fail=1
fi

[ "$fail" -eq 0 ] || exit 1
echo "check-toolchain-change-reconfigure OK — refused in place with a marker naming both compilers; reconfigured fresh onto the new one"
