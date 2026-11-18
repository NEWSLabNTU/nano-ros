#!/usr/bin/env bash
# Where does a checkout's HEAD stand against the commit it is pinned to?
# (issue 2701; the one classifier for issue 0550's submodules and issue 2697's
# west projects — a second spelling of this is how the two would drift apart.)
#
# Usage:
#   . scripts/lib/pin-drift.sh
#   nros_pin_drift <repo-dir> <pinned-commit>
#   bash scripts/lib/pin-drift.sh [<repo-dir> <pinned-commit>]   # self-test, then classify
#
# The CALLER resolves the pin to a commit (a gitlink is one already; a west
# `revision` may be a tag or a branch — `git -C <dir> rev-parse --verify
# <rev>^{commit}`). Prints exactly one line:
#
#   ok                      HEAD is the pinned commit
#   behind N                HEAD is an ancestor of the pin, N commits back
#   ahead N                 the pin is an ancestor of HEAD, N local commits on top
#   diverged                measured: neither is an ancestor of the other
#   not-verified unfetched  the pinned commit is not in this repository
#   not-verified shallow    the history is cut, so "neither" is not a measurement
#                           (scripts/lib/git-history.sh, issue 1476)
#
# HEAD only: the working tree (a lane's patch-script edits, issue 2697) is not
# drift. NEUTRAL: this says where HEAD is, never whether that is acceptable —
# a submodule AHEAD is a fork fix mid-flight, a west project ahead of a tag is
# not one of our shapes, and each caller holds its own policy table.
# Always exits 0 when it could classify; a usage error exits 2.

_nros_pin_drift_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/git-history.sh
. "$_nros_pin_drift_dir/git-history.sh"

nros_pin_drift() {
    local dir="${1:-}" pin="${2:-}" head
    if [ -z "$dir" ] || [ -z "$pin" ]; then
        echo "nros_pin_drift: usage: nros_pin_drift <repo-dir> <pinned-commit>" >&2
        return 2
    fi
    if ! git -C "$dir" cat-file -e "${pin}^{commit}" 2>/dev/null; then
        echo "not-verified unfetched"
        return 0
    fi
    head="$(git -C "$dir" rev-parse HEAD)" || return 2
    pin="$(git -C "$dir" rev-parse "${pin}^{commit}")" || return 2
    if [ "$head" = "$pin" ]; then
        echo "ok"
    elif [ "$(nros_git_ancestry "$head" "$pin" -C "$dir")" = yes ]; then
        echo "behind $(git -C "$dir" rev-list --count "${head}..${pin}")"
    elif [ "$(nros_git_ancestry "$pin" "$head" -C "$dir")" = yes ]; then
        echo "ahead $(git -C "$dir" rev-list --count "${pin}..${head}")"
    elif nros_git_history_truncated -C "$dir"; then
        echo "not-verified shallow"
    else
        echo "diverged"
    fi
}

_nros_pin_drift_self_test() {
    local t fails=0 base c1 c2 c3 side got
    t="$(mktemp -d)" || return 1
    # shellcheck disable=SC2064
    trap "rm -rf '$t'" RETURN
    # A repository this test BUILT (issue 1465): never the caller's.
    if [ -f "$_nros_pin_drift_dir/git-hook-env.sh" ]; then
        # shellcheck source=scripts/lib/git-hook-env.sh
        . "$_nros_pin_drift_dir/git-hook-env.sh"
        nros_clear_inherited_git_env
    fi
    export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t GIT_COMMITTER_EMAIL=t@t \
        GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null
    base="$t/r"
    git init -q -b main "$base"
    for n in 1 2 3; do
        echo "$n" >"$base/f"
        git -C "$base" add f
        git -C "$base" commit -q -m "c$n"
    done
    c3="$(git -C "$base" rev-parse HEAD)"
    c2="$(git -C "$base" rev-parse HEAD~1)"
    c1="$(git -C "$base" rev-parse HEAD~2)"
    git -C "$base" checkout -q -b side "$c1"
    echo s >"$base/f"
    git -C "$base" commit -q -am side
    side="$(git -C "$base" rev-parse HEAD)"

    check() {  # <want> <dir> <head> <pin>
        git -C "$2" checkout -q --detach "$3"
        got="$(nros_pin_drift "$2" "$4")"
        if [ "$got" != "$1" ]; then
            echo "pin-drift self-test FAIL: HEAD ${3:0:7} vs pin ${4:0:7}: want '$1', got '$got'" >&2
            fails=$((fails + 1))
        fi
    }
    check "ok" "$base" "$c2" "$c2"
    check "behind 2" "$base" "$c1" "$c3"
    check "ahead 2" "$base" "$c3" "$c1"
    check "diverged" "$base" "$side" "$c3"
    check "not-verified unfetched" "$base" "$c3" "0123456789abcdef0123456789abcdef01234567"
    # the working tree is not drift
    echo dirty >"$base/f"
    got="$(nros_pin_drift "$base" "$c3")"
    [ "$got" = ok ] || { echo "pin-drift self-test FAIL: a working-tree edit read as '$got'" >&2; fails=$((fails + 1)); }
    git -C "$base" checkout -q -- f
    # shallow: two unrelated-looking tips in a depth-1 clone are not "diverged"
    git clone -q --depth 1 --no-single-branch "file://$base" "$t/s" 2>/dev/null
    git -C "$t/s" fetch -q --depth 1 origin "$c3" 2>/dev/null
    check "not-verified shallow" "$t/s" "$side" "$c3"
    if nros_pin_drift "" "" >/dev/null 2>&1; then
        echo "pin-drift self-test FAIL: a usage error exited 0" >&2
        fails=$((fails + 1))
    fi
    if [ "$fails" -ne 0 ]; then
        return 1
    fi
    echo "pin-drift self-test: OK (ok, behind, ahead, diverged, unfetched, working tree, shallow, usage)"
}

# Run as a program, the self-test always runs first (a negative control nobody
# runs decays into a comment); with arguments it then classifies. Sourced, it
# only defines the function.
if [ "${BASH_SOURCE[0]}" = "$0" ]; then
    _nros_pin_drift_self_test || exit 1
    case $# in
        0) ;;
        2) nros_pin_drift "$1" "$2" ;;
        *) echo "usage: $0 [<repo-dir> <pinned-commit>]" >&2; exit 2 ;;
    esac
fi
