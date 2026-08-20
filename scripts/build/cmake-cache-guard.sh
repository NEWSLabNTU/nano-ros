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

# The compiler a configured build dir actually uses.
#
# NOT `CMAKE_C_COMPILER` from CMakeCache.txt — a cross build dir frequently has
# no such cache line at all (only `CMAKE_C_COMPILER_AR` / `_RANLIB`). The
# authoritative record is `CMakeFiles/<ver>/CMakeCCompiler.cmake`, which CMake
# writes for every configured project.
nros_cmake_dir_cc() {
    local d="$1" f
    f="$(ls "$d"/CMakeFiles/*/CMakeCCompiler.cmake 2>/dev/null | head -1)"
    if [ -n "$f" ]; then
        sed -n 's/^set(CMAKE_C_COMPILER "\([^"]*\)").*/\1/p' "$f" | head -1
        return 0
    fi
    sed -n 's/^CMAKE_C_COMPILER:[^=]*=//p' "$d/CMakeCache.txt" 2>/dev/null | head -1
}

# What `$1` (a CMake toolchain file) resolves `CMAKE_C_COMPILER` to TODAY.
#
# Issue 0706 — there is no way to read this off the file: these toolchains pick
# their compiler by searching (SDK store, then `find_program`), so the only
# authority on the answer is CMake running the file. Configure a throwaway empty
# project and read the cache.
#
# Memoized per toolchain file for the life of the shell: `fixtures-build.sh`
# calls the guard once per build dir and they share one toolchain, so this is a
# single ~1 s configure per run, not one per leaf. Empty answer on any failure —
# the caller then leaves the tree alone, which is the pre-0706 behaviour and
# never wipes on a guess.
nros_cmake_toolchain_resolved_cc() {
    local tc="$1"
    [ -n "$tc" ] && [ -f "$tc" ] || return 0

    local key cached
    key="_nros_tc_cc_$(printf '%s' "$tc" | cksum | cut -d" " -f1)"
    eval "cached=\${$key-}"
    if [ -n "${cached:-}" ]; then
        [ "$cached" = "-" ] || printf '%s\n' "$cached"
        return 0
    fi

    local probe out cc=""
    probe="$(mktemp -d 2>/dev/null)" || { eval "$key=-"; return 0; }
    printf 'cmake_minimum_required(VERSION 3.20)\nproject(nros_tc_probe C)\n' \
        > "$probe/CMakeLists.txt"
    if cmake -S "$probe" -B "$probe/b" -DCMAKE_TOOLCHAIN_FILE="$tc" \
            >/dev/null 2>&1; then
        cc="$(nros_cmake_dir_cc "$probe/b")"
    fi
    rm -rf "$probe"

    if [ -n "$cc" ]; then
        eval "$key=\$cc"
        printf '%s\n' "$cc"
    else
        eval "$key=-"
    fi
}

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

    # Rule 1b (issue 0706) — the toolchain file is the SAME file, and it now
    # resolves a DIFFERENT compiler.
    #
    # The line below used to `return 0` here, on the reasoning that "with a
    # toolchain file the compiler is pinned by that file, and rule 1 already
    # covers a change to it". Rule 1 covers a change to the file's PATH. It
    # cannot see a change in what that path RESOLVES to — and these toolchain
    # files resolve: `_nros_riscv64_find_prefix` searches the SDK store first
    # and falls back to `find_program`, so installing `riscv-none-elf-gcc` into
    # the store silently moves the answer from Debian's picolibc gcc to xPack's
    # newlib one with the argument byte-identical. The guard saw no change, kept
    # the tree, and every compile went on using the old compiler.
    #
    # What that cost: issue 0680's newlib-only `reent.c` compiled against
    # picolibc headers — `fatal error: sys/reent.h: No such file or directory` —
    # on a tree whose own configure had just printed `libc = newlib`, because
    # the verdict came from the resolver and the compile came from the cache.
    # It failed the whole of tier 2 (1-wise over platform) and read as a code
    # bug for hours.
    #
    # So ASK the toolchain file rather than assume it is stable, which is the
    # same rule RFC-0076 applies to the platform ABI and issue 0570 to the
    # storage probes. The probe is one real `cmake` configure of an empty
    # project, memoized per toolchain file per shell, so a fan-out over many
    # build dirs pays it once.
    if [ -n "$cached_tc" ]; then
        local want_cc cached_cc
        want_cc="$(nros_cmake_toolchain_resolved_cc "$cached_tc")"
        cached_cc="$(nros_cmake_dir_cc "$build_dir")"
        if [ -n "$want_cc" ] && [ -n "$cached_cc" ] && [ "$want_cc" != "$cached_cc" ]; then
            echo "  (toolchain RESOLUTION change: '$cached_cc' -> '$want_cc' — wiping $build_dir;" >&2
            echo "   same toolchain file, different compiler; a re-configure cannot move it, issue 0706)" >&2
            rm -rf "$build_dir"
        fi
        return 0
    fi

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
