#!/usr/bin/env bash
# Shared CMake build-dir invalidation rules, sourced by `cmake-incremental.sh`
# and `fixture-matrix.sh`.
#
# WHY THIS IS ONE FILE. Both scripts already carried a byte-identical copy of
# the toolchain-file rule (issue 0391), one of them pointing at the other for
# its rationale. A second rule was about to be pasted into both, which is the
# pattern this repo keeps paying for (CLAUDE.md "fix the CLASS … add ONE shared
# helper rather than a second spelling"). New rules go here and both callers
# get them.
#
# THE RULES, and what each is defending against:
#
#  1. TOOLCHAIN FILE (issue 0391). CMake pins the compiler at FIRST configure; a
#     later `-DCMAKE_TOOLCHAIN_FILE=` does not swap it. A dir first configured
#     host-cc stays host-cc forever and silently feeds a RISC-V/ARM kernel to
#     the x86 assembler ("Error: no such instruction: csrrci %rax,mstatus").
#
#  2. COMPILER VERSION (issue 0400). CMake caches `check_symbol_exists` results
#     as INTERNAL entries and a reconfigure NEVER re-tests them. So a build dir
#     configured on the host and re-entered from the ROS distrobox keeps the
#     HOST's answers about the box's libc. Concretely: glibc grew `strlcpy` /
#     `strlcat` in 2.38, the Arch host has them, Ubuntu 22.04 (2.35) does not —
#     CycloneDDS's idlc therefore skipped its own fallback and the link died on
#
#         mbchar.c:(.text.set_encoding+0x80): undefined reference to `strlcpy'
#
#     in vendored code that compiles fine on both machines. The compiler VERSION
#     is the cheapest discriminator that separates host from container (gcc 16
#     vs 11), and it also catches a plain host toolchain upgrade, where stale
#     capability probes are the same hazard.
#
# Both rules WIPE rather than reconfigure, because neither a pinned compiler nor
# a cached probe result can be moved by re-running cmake.

# Wipe $1 when its CMakeCache was produced under a different toolchain file or a
# different C compiler version. Remaining args are the configure args (scanned
# for -DCMAKE_TOOLCHAIN_FILE=).
nros_cmake_guard_build_dir() {
    local build_dir="$1"; shift
    [ -f "$build_dir/CMakeCache.txt" ] || return 0

    local want_tc="" cached_tc _a
    for _a in "$@"; do
        case "$_a" in
            -DCMAKE_TOOLCHAIN_FILE=*) want_tc="${_a#-DCMAKE_TOOLCHAIN_FILE=}" ;;
        esac
    done
    cached_tc="$(sed -n 's/^CMAKE_TOOLCHAIN_FILE:[^=]*=//p' "$build_dir/CMakeCache.txt")"
    if [ "$want_tc" != "$cached_tc" ]; then
        echo "  (toolchain change: '${cached_tc:-<none>}' -> '${want_tc:-<none>}' — wiping $build_dir)" >&2
        rm -rf "$build_dir"
        return 0
    fi

    # Only meaningful for a HOST-cc build dir: with a toolchain file the
    # compiler is pinned by that file, and rule 1 already covers a change to it.
    [ -n "$cached_tc" ] && return 0

    local cached_cc_ver="" now_cc_ver="" cc_info
    cc_info="$(ls "$build_dir"/CMakeFiles/*/CMakeCCompiler.cmake 2>/dev/null | head -1)"
    [ -n "$cc_info" ] || return 0
    cached_cc_ver="$(sed -n 's/^set(CMAKE_C_COMPILER_VERSION "\([^"]*\)").*/\1/p' "$cc_info" | head -1)"
    [ -n "$cached_cc_ver" ] || return 0

    now_cc_ver="$("${CC:-cc}" -dumpfullversion 2>/dev/null || "${CC:-cc}" -dumpversion 2>/dev/null || true)"
    [ -n "$now_cc_ver" ] || return 0

    if [ "$cached_cc_ver" != "$now_cc_ver" ]; then
        echo "  (C compiler change: $cached_cc_ver -> $now_cc_ver — wiping $build_dir;" >&2
        echo "   cached check_symbol_exists results describe the OTHER environment, issue 0400)" >&2
        rm -rf "$build_dir"
    fi
}
