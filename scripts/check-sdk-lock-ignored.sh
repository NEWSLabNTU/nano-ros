#!/usr/bin/env bash
# Issue 1845 -- every `nros-sdk.lock` a build can write in this checkout is
# IGNORED.
#
# `nros build` writes `<ws>/nros-sdk.lock` (RFC-0104 D4). In a checkout that
# file is a build artifact, and `.gitignore` covered only the root one
# (`/nros-sdk.lock`). So each example workspace's lock was untracked and
# unignored, which put it in `nros_source_manifest`
# (`git ls-files --cached --others --exclude-standard`). Each entry build
# appends its own section, so every sibling workspace fixture built before it
# read STALE. That was 23 of 35 tests in one local sweep, and 4 live-peer host
# cells in run 38321018350, minutes after the job built them.
#
# The rule is about every directory a build can write a lock into: a
# workspace (`.colcon_workspace`) or a project that states its images
# (`system.toml`). It asks git, so it agrees with the manifest by
# construction.
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/lib/git-hook-env.sh
. "$repo_root/scripts/lib/git-hook-env.sh"

# check <repo>: prints each lock path git would NOT ignore; status 1 if any.
check() {
    local repo="$1" dir bad=0
    while IFS= read -r dir; do
        if ! git -C "$repo" check-ignore -q "$dir/nros-sdk.lock"; then
            echo "  $dir/nros-sdk.lock"
            bad=1
        fi
    done < <(git -C "$repo" ls-files -- '*system.toml' '*.colcon_workspace' \
        | xargs -r -n1 dirname | sort -u)
    return "$bad"
}

# The negative control, on a repository this gate builds: the pre-1845 rule
# (`/nros-sdk.lock`, root only) must be caught, and the fixed one must pass.
selftest() {
    local tmp
    tmp="$(mktemp -d)"
    trap 'rm -rf "$tmp"' RETURN
    (
        nros_clear_inherited_git_env
        cd "$tmp"
        git init -q .
        mkdir -p ws/a
        : > ws/.colcon_workspace
        : > ws/a/system.toml
        printf '/nros-sdk.lock\n' > .gitignore
        git add -A
    )
    if (nros_clear_inherited_git_env; check "$tmp") >/dev/null; then
        echo "check-sdk-lock-ignored: SELFTEST FAILED -- a root-only rule was not caught" >&2
        return 1
    fi
    printf 'nros-sdk.lock\n' > "$tmp/.gitignore"
    if ! (nros_clear_inherited_git_env; check "$tmp") >/dev/null; then
        echo "check-sdk-lock-ignored: SELFTEST FAILED -- the any-depth rule was refused" >&2
        return 1
    fi
}
selftest

if ! out="$(check "$repo_root")"; then
    echo "check-sdk-lock-ignored: FAIL -- a build would write these locks untracked and" >&2
    echo "  unignored, which makes them fixture INPUTS (issue 1845):" >&2
    printf '%s\n' "$out" >&2
    echo "  Remedy: keep \`nros-sdk.lock\` (any depth) in the root .gitignore." >&2
    exit 1
fi
echo "check-sdk-lock-ignored: OK -- every workspace/project lock is ignored"
