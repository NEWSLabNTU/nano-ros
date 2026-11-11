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
#  3. FOREIGN CHECKOUT (issue 1840). A cache configured against ANOTHER
#     nano-ros checkout's runtime (an inherited `nano_ros_ROOT`) keeps that
#     checkout in every cached path and in non-path answers too. Drops
#     `CMakeCache.txt` only; see `nros_cmake_guard_foreign_checkout`.
#
#  4. RETIRED NAME (phase-484 W8). A cache first configured by an older
#     nano-ros carries the retired `NANO_ROS_*` entries that tree wrote itself
#     (`NANO_ROS_PLATFORM:STRING`, `NANO_ROS_RMW:STRING`, …). The retired names
#     are now a configure ERROR, so such a dir could never reconfigure — and
#     ninja re-runs cmake on its own whenever a CMakeLists changes. Every caller
#     of this file configures with `NROS_*` names only
#     (`check-retired-cmake-names`), so an old name in its cache is residue,
#     never a choice: drops those entries only; see
#     `nros_cmake_guard_retired_names`.
#
#  5. STALE MAKE (issue 2421). A fan-out pool (`jobserver-pool.sh`) hands its
#     units `--jobserver-auth=fifo:`, and a build dir configured before the
#     pinned make came first on PATH (issue 2119) keeps running the make its
#     cache recorded (`CMAKE_MAKE_PROGRAM`, or a `find_program(MAKE)` result
#     such as Zephyr native_sim's). A make 4.3 stops on `invalid
#     --jobserver-auth`, the class issue 2241 fixed for `nros build`. Re-running
#     `cmake <dir>` does not refresh a found entry, so it is REWRITTEN to the
#     make on PATH by the same helper `nros build` reads caches with
#     (`nros dev jobserver repair-makes`); see `nros_cmake_guard_stale_make`.
#
# Rules 1 and 2 WIPE rather than reconfigure, because neither a pinned compiler nor
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
    nros_cmake_guard_retired_names "$build_dir"
    nros_cmake_guard_stale_make "$build_dir"

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

# Rule 3 (issue 1840) — a cache BOUND TO ANOTHER nano-ros checkout.
#
# `$1` is a build dir, `$2` the checkout the build belongs to. A configure run
# with an inherited `nano_ros_ROOT` naming another checkout (a linked worktree
# whose parent shell activated the main checkout) records that checkout across
# the WHOLE cache — `nano_ros_DIR`, `NROS_PLATFORM_CFFI_INCLUDE`,
# `CYCLONEDDS_SOURCE_DIR`, ~25 `_NROS_*:INTERNAL` module dirs, and non-path
# answers its older cmake gave (the RMW under its deprecated alias, which this checkout's
# `NanoRosDeprecatedNames.cmake` then refuses as disagreeing). `find_package`
# never re-searches a found `nano_ros_DIR`, so a later, correct configure keeps
# every one of them: the cache cannot converge, the same reason rules 1/2 exist.
# The non-path entries are why the repair is the CACHE and not a list of
# `-U` names — they are not attributable to a checkout one by one.
#
# "Another checkout" is the ONE rule (`checkout-paths.sh`), applied to every
# absolute value: an out-of-tree SDK, the system, the store, and this checkout
# are all kept. Only `CMakeCache.txt` goes — objects and `.ninja_log` stay, so
# ninja recompiles exactly what the reconfigure changed.
nros_cmake_guard_foreign_checkout() {
    local build_dir="$1" here="$2" cache="$1/CMakeCache.txt"
    [ -f "$cache" ] && [ -n "$here" ] || return 0
    local name value first="" n=0
    while IFS=$'\t' read -r name value; do
        [ "$(nros_reroot_checkout_path "$value" "$here")" = "$value" ] && continue
        n=$((n + 1))
        [ -n "$first" ] || first="$name=$value"
    done < <(sed -n 's/^\([A-Za-z_][^:=]*\):[A-Z]*=\(\/.*\)$/\1\t\2/p' "$cache" | sort -u -t$'\t' -k2,2)
    [ "$n" -gt 0 ] || return 0
    echo "  (cache bound to another nano-ros checkout: $n value(s), first $first —" >&2
    echo "   dropping $cache so it reconfigures against $here; issue 1840)" >&2
    rm -f "$cache"
}

# Rule 4 (phase-484 W8) — drop RETIRED `NANO_ROS_*` entries from a cache.
#
# The names are READ from the error table (`cmake/NanoRosRetiredNames.cmake`),
# never re-spelled here. The table path is resolved when this file is sourced
# and exported, because `fixtures-build.sh` ships these functions to make
# workers with `export -f`, where `BASH_SOURCE` no longer names this file.
# Only the cache lines go: a configure that still PASSES an old name is
# refused by cmake as before.
_NROS_RETIRED_NAMES_TABLE="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)/cmake/NanoRosRetiredNames.cmake"
export _NROS_RETIRED_NAMES_TABLE
nros_cmake_guard_retired_names() {
    local cache="$1/CMakeCache.txt" names hits
    [ -f "$cache" ] && [ -f "${_NROS_RETIRED_NAMES_TABLE:-}" ] || return 0
    names="$(sed -n 's/^[[:space:]]*\(NANO_ROS_[A-Z0-9_]*\)=NROS_.*/\1/p' "$_NROS_RETIRED_NAMES_TABLE" | paste -sd'|')"
    [ -n "$names" ] || return 0
    hits="$(grep -Eo "^($names):" "$cache" | tr -d : | paste -sd' ')" || true
    [ -n "$hits" ] || return 0
    echo "  (cache carries retired name(s) $hits from an older configure of $1 —" >&2
    echo "   dropping those entries; phase-484 W8)" >&2
    # An entry and its `//` help lines go together: CMake refuses a cache
    # whose help comment is followed by no entry ("Parse error in cache
    # file"), so deleting the entry line alone breaks the dir it repairs.
    awk -v re="^(${names}):" '
        /^\/\// { buf = buf $0 "\n"; next }
        $0 ~ re  { buf = ""; next }
        { printf "%s", buf; buf = ""; print }
        END { printf "%s", buf }' "$cache" > "$cache.nros-retired.tmp" \
        && mv "$cache.nros-retired.tmp" "$cache"
}

# Rule 5 (issue 2421) — a cache whose recorded make cannot join the FIFO
# jobserver this unit was handed.
#
# Acts only under a FIFO pool (`MAKEFLAGS` names `--jobserver-auth=fifo:`):
# outside one, an old make is harmless, and rewriting it would only churn the
# cache. `$1` is the build dir; its immediate subdirectories' caches (a
# sysbuild's domains) are covered too. The CLI does the reading, version
# probing and rewrite, so the shell and `nros build` (issue 2241) share one
# definition of "too old". Without a CLI that knows the verb, it says so and
# leaves the dir alone.
#
# The CLI comes from `nros_cli_bin` (cargo.sh), sourced here when the caller
# did not: its path is resolved when this file is sourced and exported, as
# rule 4's table is, since a make worker's `export -f` copy has no
# `BASH_SOURCE` naming this file.
_NROS_CACHE_GUARD_CARGO_SH="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/cargo.sh"
export _NROS_CACHE_GUARD_CARGO_SH
nros_cmake_guard_stale_make() {
    local d="$1" cli=""
    case "${MAKEFLAGS:-}" in *--jobserver-auth=fifo:*) ;; *) return 0 ;; esac
    [ -d "$d" ] || return 0
    if ! declare -F nros_cli_bin >/dev/null 2>&1 \
        && [ -f "${_NROS_CACHE_GUARD_CARGO_SH:-}" ]; then
        # shellcheck source=scripts/build/cargo.sh
        . "$_NROS_CACHE_GUARD_CARGO_SH" >/dev/null 2>&1 || true
    fi
    if declare -F nros_cli_bin >/dev/null 2>&1; then
        cli="$(nros_cli_bin 2>/dev/null)" || cli=""
    fi
    if [ -z "$cli" ] || ! "$cli" dev jobserver repair-makes "$d" >&2; then
        echo "  (could not check $d for a cached make older than 4.4; a FIFO pool" >&2
        echo "   unit it runs may stop on 'invalid --jobserver-auth', issue 2421)" >&2
    fi
    return 0
}
