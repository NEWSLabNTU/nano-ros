#!/usr/bin/env bash
# Issue 2958 -- `nros_prepush_tree_matches` (scripts/lib/prepush-tree.sh), the
# pre-push refusal of a push whose working tree differs from the commit it
# publishes, driven case by case in a throwaway repository. Each case is one of
# the shapes the issue names, plus the controls that must pass.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
# shellcheck source=scripts/lib/git-hook-env.sh
. "$root/scripts/lib/git-hook-env.sh"
nros_clear_inherited_git_env
# shellcheck source=scripts/lib/prepush-tree.sh
. "$root/scripts/lib/prepush-tree.sh"

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT
fail=0

# A fresh repository with one commit: a file, a doc, and an ignore rule.
fresh() {
    rm -rf "$work/r"
    git init -q "$work/r"
    git -C "$work/r" config user.email t@t
    git -C "$work/r" config user.name t
    printf 'a\n' > "$work/r/a.txt"
    mkdir -p "$work/r/docs"
    printf 'status: open\n' > "$work/r/docs/i.md"
    printf 'ignored/\n' > "$work/r/.gitignore"
    git -C "$work/r" add -A
    git -C "$work/r" commit -q -m init
}

# expect <name> <0|1> <ref-line>: run the check on the current repo state.
expect() {
    local name="$1" want="$2" line="$3" got=0
    printf '%s\n' "$line" | nros_prepush_tree_matches "$work/r" 2>/dev/null || got=1
    if [ "$got" != "$want" ]; then
        echo "check-prepush-tree: FAIL $name: wanted $want, got $got" >&2
        fail=1
    fi
}

head_line() { echo "refs/heads/b $(git -C "$work/r" rev-parse HEAD) refs/heads/b 0000000000000000000000000000000000000000"; }

# The helper's cases, run on every invocation: this gate IS its selftest.
selftest() {

    fresh
    expect "a clean tree pushes" 0 "$(head_line)"

    fresh; printf 'b\n' >> "$work/r/a.txt"
    expect "an unstaged edit is refused" 1 "$(head_line)"

    fresh; printf 'b\n' >> "$work/r/a.txt"; git -C "$work/r" add a.txt
    expect "a staged, uncommitted edit is refused" 1 "$(head_line)"

    fresh; printf 'n\n' > "$work/r/new.txt"
    expect "an untracked file is refused" 1 "$(head_line)"

    fresh; rm "$work/r/a.txt"
    expect "an unstaged deletion is refused" 1 "$(head_line)"

    # The incident: `git mv` stages the INDEX copy, so an edit made before it
    # stays unstaged and the commit archives the old text.
    fresh; printf 'status: resolved\n' > "$work/r/docs/i.md"
    mkdir -p "$work/r/docs/archived"
    git -C "$work/r" mv docs/i.md docs/archived/i.md
    git -C "$work/r" commit -q -m archive
    expect "git mv over an unstaged edit is refused (PR #4821)" 1 "$(head_line)"

    fresh; mkdir -p "$work/r/ignored"; printf 'x\n' > "$work/r/ignored/x"
    expect "an ignored file is not a difference" 0 "$(head_line)"

    fresh; printf 'b\n' >> "$work/r/a.txt"
    other="$(git -C "$work/r" rev-parse HEAD)"
    git -C "$work/r" stash -q
    git -C "$work/r" commit -q --allow-empty -m next
    git -C "$work/r" stash pop -q
    expect "pushing a commit other than HEAD is not judged" 0 \
        "refs/heads/old $other refs/heads/old 0000000000000000000000000000000000000000"

    fresh; printf 'b\n' >> "$work/r/a.txt"
    expect "a tag push is not judged" 0 \
        "refs/tags/t $(git -C "$work/r" rev-parse HEAD) refs/tags/t 0000000000000000000000000000000000000000"

    fresh; printf 'b\n' >> "$work/r/a.txt"
    expect "no ref lines (git pushes nothing) is not judged" 0 ""
}

selftest

if [ "$fail" -ne 0 ]; then
    exit 1
fi
echo "check-prepush-tree: OK (10 cases)"
