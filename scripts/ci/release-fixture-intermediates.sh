#!/usr/bin/env bash
#
# Issue 2130 — release a fixture build's COMPILE intermediates once its
# binaries are linked, on a CI lane that must keep the fixtures themselves.
#
# `live-peer`'s host job builds every native fixture its cells resolve on a
# GitHub-hosted runner, and it ran out of disk before the cells finished: the
# builds took 106 G free down below 49 G. Issue 2100's lever (delete a gate's
# whole output once it has a verdict) does not apply here, because the cells
# still need the fixtures afterwards. What they do not need is what was only
# ever an input to a LINK:
#
#   <cargo target>/…/deps/*.rlib, *.rmeta   crate libraries a link consumed
#   <cargo target>/…/incremental/           rustc's incremental cache
#   <cmake build>/…/CMakeFiles/**/*.o       object files a link consumed
#   <cargo target>/…/build/<crate>-<hash>/build{-,_}script{-,_}build*
#                                            a build script's own executable
#
# The last row is issue 2469's. A build script runs during the build and never
# again: what the cells and the probes read is its `output` and `out/`, both
# kept. Its executable is the bulk of what a C/C++ example row keeps, because
# the host half of every cargo dir carries debuginfo (issue 2318): measured on
# `examples/linux/c/talker`, 755 MB of the 1.2 G left after the other rows.
# Cargo rebuilds one if a later build of that target needs it.
#
# Everything the cells and the freshness probes read is KEPT: every binary,
# every `.a`/`.so`, every make-style `.d` and `.o.d` dep-info file, every build
# script's `output`, and `.ninja_deps`. The probes read dep-info and ninja's
# log, never a crate library or an object (`fixtures/binaries/mod.rs` skips
# `.fingerprint`, `incremental` and `CMakeFiles` in its walks, and a dep-info
# path that no longer exists is not an edit event).
#
# WHY `rm` IS RIGHT HERE, since `rm -rf` + rebuild is an antipattern in this
# tree: that rule is about wiping a build to make an incremental build behave.
# This runs on a cold CI runner after the build is done, and nothing on that
# runner compiles again. A later build that shares one of these target dirs
# recompiles what it needs, which costs time and changes no artifact.
#
# It never fails the job: a release that can break the lane it is helping is
# worse than none, the same rule `reclaim-disk.sh` states.
#
# Usage:
#   scripts/ci/release-fixture-intermediates.sh [--dry-run] <dir>...
#   scripts/ci/release-fixture-intermediates.sh --self-test
#
# The byte counts are INTEGERS from `du` itself, never through `awk` (issue
# 2469). `awk 'END {print $1+0}'` printed a root's total as `1.74022e+10` under
# mawk, the CI image's awk, once it passed 2^31 bytes. Bash arithmetic refused
# that, which aborted the loop after the FIRST root: live-peer's host job
# released `build/` and never reached the example build dirs, and reported
# "released 0 MiB" (run 42957501616). gawk prints the integer, so it never
# failed on a developer host.
#
# A directory outside this checkout is refused, never entered.
set -uo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"

# Issue 2469 — two roots, the first holding a 3 GiB SPARSE crate library (du -b
# counts the apparent size, so it costs no disk) and the second a small one.
# Both must be released and reported, and the total must be an integer: the
# old `awk` count failed exactly here under mawk and never reached root two.
if [ "${1:-}" = "--self-test" ]; then
    mkdir -p "$repo/tmp"
    t="$(mktemp -d "$repo/tmp/release-selftest.XXXXXX")" || exit 1
    trap 'rm -rf "$t"' EXIT
    mkdir -p "$t/a/x86_64/deps" "$t/b/proj/CMakeFiles/obj.dir" "$t/b/proj/bin" \
        "$t/b/proj/target/release/build/zpico-sys-0123456789abcdef/out"
    truncate -s 3G "$t/a/x86_64/deps/libbig-0123456789abcdef.rlib"
    printf 'obj' > "$t/b/proj/CMakeFiles/obj.dir/main.c.o"
    printf 'bin' > "$t/b/proj/bin/app"
    printf 'bs' > "$t/b/proj/target/release/build/zpico-sys-0123456789abcdef/build-script-build"
    printf 'bs' > "$t/b/proj/target/release/build/zpico-sys-0123456789abcdef/build_script_build-0123456789abcdef"
    printf 'cargo:rustc-link-lib=x' > "$t/b/proj/target/release/build/zpico-sys-0123456789abcdef/output"
    printf 'h' > "$t/b/proj/target/release/build/zpico-sys-0123456789abcdef/out/gen.h"
    out="$(bash "$0" "$t/a" "$t/b" 2>&1)"
    fails=0
    check() { if eval "$1"; then echo "  ok   $2"; else echo "  FAIL $2" >&2; fails=$((fails + 1)); fi; }
    check '[ ! -e "$t/a/x86_64/deps/libbig-0123456789abcdef.rlib" ]' "the first root's 3 GiB library is released"
    check '[ ! -e "$t/b/proj/CMakeFiles/obj.dir/main.c.o" ]' "the SECOND root is reached and its object released"
    check '[ -e "$t/b/proj/bin/app" ]' "a binary is kept"
    bsd="$t/b/proj/target/release/build/zpico-sys-0123456789abcdef"
    check '[ ! -e "$bsd/build-script-build" ] && [ ! -e "$bsd/build_script_build-0123456789abcdef" ]' \
        "a build script's executables are released"
    check '[ -e "$bsd/output" ] && [ -e "$bsd/out/gen.h" ]' "a build script's output and out/ are kept"
    check 'grep -qE "released +3072 MiB" <<<"$out"' "the 3 GiB root reports 3072 MiB"
    check 'grep -qE "released 3072 MiB of compile intermediates in total" <<<"$out"' "the total is an integer"
    check '! grep -q "syntax error" <<<"$out"' "no arithmetic error"
    if [ "$fails" -ne 0 ]; then
        printf '%s\n' "$out" | sed 's/^/    | /' >&2
        echo "release-fixture-intermediates --self-test: $fails case(s) FAILED" >&2
        exit 1
    fi
    echo "release-fixture-intermediates --self-test: OK"
    exit 0
fi

dry=0
if [ "${1:-}" = "--dry-run" ]; then
    dry=1
    shift
fi

total=0
for d in "$@"; do
    [ -d "$d" ] || continue
    real="$(cd "$d" 2>/dev/null && pwd -P)" || continue
    case "$real/" in
        "$repo"/?*/) ;;
        *)
            echo "release-fixture-intermediates: refusing $d — not inside $repo" >&2
            continue
            ;;
    esac
    # -print0 so a path with a space is one path; `du --files0-from` prices
    # what goes before it goes.
    list="$(mktemp)"
    find "$real" \
        \( -type f -path '*/deps/*' \( -name '*.rlib' -o -name '*.rmeta' \) -print0 \) -o \
        \( -type f -path '*/incremental/*' -print0 \) -o \
        \( -type f -path '*/CMakeFiles/*' -name '*.o' -print0 \) -o \
        \( -type f -regextype posix-extended \
            -regex '.*/build/[^/]+-[0-9a-f]{16}/build[-_]script[-_]build[^/]*' -print0 \) \
        2>/dev/null >"$list"
    bytes=0
    if [ -s "$list" ]; then
        bytes="$(du -cb --files0-from="$list" 2>/dev/null | tail -n 1 | cut -f1)"
        case "$bytes" in '' | *[!0-9]*) bytes=0 ;; esac
        [ "$dry" = 1 ] || xargs -0 rm -f -- <"$list"
    fi
    rm -f "$list"
    total=$((total + bytes))
    printf '  %s %6d MiB  %s\n' "$([ "$dry" = 1 ] && echo would-release || echo released)" \
        $((bytes / 1048576)) "${real#"$repo"/}"
done
printf '  %s %d MiB of compile intermediates in total\n' \
    "$([ "$dry" = 1 ] && echo would-release || echo released)" $((total / 1048576))
exit 0
