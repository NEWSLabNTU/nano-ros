#!/usr/bin/env bash
# Sourced by fixture builders; run directly only for `selftest`.
#
# A fixture build must not leave a file in its source dir that git neither
# tracks nor ignores.
#
# Issue 1862. A fixture's `.inputsig` hashes its dir through
# `nros_source_manifest` (`git ls-files --cached --others --exclude-standard`).
# It counts untracked files on purpose, because an uncommitted source is an
# input. But a file the BUILD writes there is then an input too. `nros build`
# appends one section per entry to `<workspace>/nros-sdk.lock`, and before
# issue 1845 ignored it, each row's build moved the signature of every row
# built before it in the same workspace. Live-peer run 38321018350 read four
# cells STALE minutes after the job built them, and only the last-built row in
# each workspace stayed fresh.
#
# Issues 1845 and 1866 added ignore rules for the two writers known so far
# (the lock and the cmake build trees). This asks about what a build actually
# wrote: the builder snapshots the dir before a row and refuses the row if the
# build added an unignored path. A file that was already there before the
# build (a developer's uncommitted source) is never blamed.
#
#   nros_unignored_snapshot <repo_root> <rel_dir>
#       Prints the dir's untracked, unignored paths, sorted, one per line.
#   nros_refuse_unignored_writes <repo_root> <rel_dir> <before_file> <who>
#       Status 1, naming each path, if the dir now holds one that
#       <before_file> (a snapshot) did not.

nros_unignored_snapshot() {
    local repo_root="${1:?nros_unignored_snapshot: repo_root}"
    local rel_dir="${2:?nros_unignored_snapshot: rel_dir}"
    git -C "$repo_root" ls-files --others --exclude-standard -- "$rel_dir" | LC_ALL=C sort
}

nros_refuse_unignored_writes() {
    local repo_root="${1:?nros_refuse_unignored_writes: repo_root}"
    local rel_dir="${2:?nros_refuse_unignored_writes: rel_dir}"
    local before="${3:?nros_refuse_unignored_writes: before_file}"
    local who="${4:?nros_refuse_unignored_writes: who}"
    local after added
    after="$(nros_unignored_snapshot "$repo_root" "$rel_dir")" || return 1
    added="$(LC_ALL=C comm -13 "$before" <(printf '%s\n' "$after" | sed '/^$/d'))" || return 1
    [ -z "$added" ] && return 0
    {
        echo "ERROR: $who: the build wrote file(s) that git neither tracks nor ignores:"
        printf '%s\n' "$added" | sed 's/^/         /'
        echo "       The fixture signature hashes every unignored file under $rel_dir,"
        echo "       so each one re-stales every fixture built from this dir before it"
        echo "       (issue 1862). Add a .gitignore rule for what the build writes."
    } >&2
    return 1
}

# The negative control, on a repository this builds: a write the snapshot did
# not hold is refused, an ignored write and a pre-existing file are not.
_nros_unignored_writes_selftest() {
    local here tmp
    here="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
    # shellcheck source=scripts/lib/git-hook-env.sh
    . "$here/scripts/lib/git-hook-env.sh"
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' RETURN
    (
        nros_clear_inherited_git_env
        set -e
        cd "$tmp"
        git init -q .
        mkdir -p ws/src
        : > ws/src/lib.rs
        printf 'ws/build/\n' > .gitignore
        git add .gitignore ws/src/lib.rs
        : > ws/uncommitted.rs
        nros_unignored_snapshot "$tmp" ws > before
        mkdir -p ws/build
        : > ws/build/out.o
        nros_refuse_unignored_writes "$tmp" ws before selftest 2>/dev/null || {
            echo "unignored-writes: SELFTEST FAILED -- an ignored write or a pre-existing file was refused" >&2
            exit 1
        }
        : > ws/nros-sdk.lock
        if nros_refuse_unignored_writes "$tmp" ws before selftest 2>/dev/null; then
            echo "unignored-writes: SELFTEST FAILED -- an unignored write was not refused" >&2
            exit 1
        fi
    )
}

if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    set -euo pipefail
    case "${1:-}" in
        selftest)
            _nros_unignored_writes_selftest
            echo "unignored-writes: selftest OK (a new unignored write is refused; ignored and pre-existing files are not)"
            ;;
        *)
            echo "usage: $0 selftest" >&2
            exit 2
            ;;
    esac
fi
