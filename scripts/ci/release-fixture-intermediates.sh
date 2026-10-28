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
#
# A directory outside this checkout is refused, never entered.
set -uo pipefail

repo="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd -P)"
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
        \( -type f -path '*/CMakeFiles/*' -name '*.o' -print0 \) \
        2>/dev/null >"$list"
    bytes=0
    if [ -s "$list" ]; then
        bytes="$(du -cb --files0-from="$list" 2>/dev/null | awk 'END {print $1+0}')"
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
