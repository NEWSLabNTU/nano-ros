# What a git ancestry answer is worth when the checkout's history is cut.
#
# Shell twin of `scripts/lib/git_history.py`, same names and the same rule.
# The Python file carries the reasoning and the measurement (issue 1476); this
# one exists because two of the call sites are shell and a second hand-written
# spelling of the rule is how the first one drifted.
#
# Truncation can only manufacture a FALSE NEGATIVE: if git finds a path, the
# path exists. So a `yes` counts from any clone, and a `no` from a shallow one
# becomes `unknown` — which the caller must REPORT, never read as a verdict.
#
# Usage:
#   . scripts/lib/git-history.sh
#   nros_git_history_truncated -C "$path"            # rc 0 when shallow
#   nros_git_ancestry <a> <b> [git-locator-args...]  # echoes yes|no|unknown
#   (old trees and blobs: see "History OBJECT reads" at the end)
#
# The locator args are whatever names the repository to ask — `-C <path>`,
# `--git-dir=<store>`, or nothing for the current directory.

# rc 0 when this checkout's COMMIT HISTORY is cut (a shallow clone). A PARTIAL
# clone (`--filter=blob:none`) has every commit and is deliberately NOT this.
nros_git_history_truncated() {
    [ "$(git "$@" rev-parse --is-shallow-repository 2>/dev/null)" = "true" ]
}

# Is <a> in <b>'s history, as far as this checkout can tell?
#
#   yes      measured: git walked a path from <b> back to <a>
#   no       measured: it walked the whole history and found none
#   unknown  NOT measured: the history is truncated here, so the negative
#            above is not evidence. Say so; do not fail on it.
nros_git_ancestry() {
    local a="${1:?nros_git_ancestry: <a>}" b="${2:?nros_git_ancestry: <b>}"
    shift 2
    if git "$@" merge-base --is-ancestor "$a" "$b" 2>/dev/null; then
        echo yes
        return 0
    fi
    if nros_git_history_truncated "$@"; then
        echo unknown
        return 0
    fi
    echo no
}

# ---------------------------------------------------------------------------
# History OBJECT reads (issue 2442). The Python file carries the reasoning:
# a PARTIAL clone (CI's `--filter=tree:0 --unshallow`) is not shallow, so the
# predicate above lets it through, and any read of an OLD tree or blob there
# fetches it lazily from the network, one object per round trip. Same names
# as the Python helper, same rule.
#
#   nros_git_partial [locator...]                 # rc 0 when partial
#   nros_git_history_objects_unreadable [loc...]  # rc 0 + reason on stdout
#                                                 #   when old trees cannot
#                                                 #   be read here
#   nros_git_tree_present <rev> [locator...]      # rc 0 when <rev>'s whole
#                                                 #   tree is HERE (no fetch)
#   nros_git_history_run <timeout-s> <git args...> # one bounded read; rc 124
#                                                 #   when it ran out of time
#
# An unreadable or timed-out answer is NOT VERIFIED (nros_check_skip /
# nros_check_unverified), never a pass.

NROS_GIT_HISTORY_TIMEOUT_S=60
# shellcheck source=scripts/lib/deadline.sh
. "$(dirname "${BASH_SOURCE[0]}")/deadline.sh"

# rc 0 when some objects live only on a promisor remote.
nros_git_partial() {
    local out
    out="$(GIT_TERMINAL_PROMPT=0 git "$@" config --get-regexp \
        '^(extensions\.partialclone|remote\..*\.promisor)$' </dev/null 2>/dev/null)" || return 1
    [ -n "$out" ]
}

# rc 0, with the reason on stdout, when a read of old trees/blobs across
# history cannot be answered in this checkout. rc 1 when it can.
nros_git_history_objects_unreadable() {
    if nros_git_history_truncated "$@"; then
        echo "this clone is shallow, so history before its tip is absent"
        return 0
    fi
    if nros_git_partial "$@"; then
        echo "this clone is partial (trees or blobs live on a promisor remote), and reading old trees would fetch them one object at a time"
        return 0
    fi
    return 1
}

# rc 0 when <rev>'s whole tree (every subtree; blobs not asked) is here.
# Locator args after <rev> name the repository, as for the functions above.
# `rev-list --missing=print` reports a missing object instead of fetching it;
# `cat-file -e <rev>^{tree}` would fetch it.
nros_git_tree_present() {
    local rev="${1:?nros_git_tree_present: <rev>}" out
    shift
    out="$(GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 "${NROS_DEADLINE[@]}" "${NROS_GIT_HISTORY_TIMEOUT_S}" \
        git "$@" rev-list --objects --no-object-names --missing=print \
        --filter=blob:none --no-walk "$rev" </dev/null 2>/dev/null)" || return 1
    ! grep -q '^?' <<<"$out"
}

# One bounded git read: no prompt, no lazy fetch where git honours it, stdin
# closed, a deadline. Exit status is git's, or 124 when the deadline hit.
nros_git_history_run() {
    local t="${1:?nros_git_history_run: <timeout-s>}"
    shift
    GIT_TERMINAL_PROMPT=0 GIT_NO_LAZY_FETCH=1 \
        "${NROS_DEADLINE[@]}" "$t" git "$@" </dev/null
}

# Self-test: the same full / shallow / partial negative control as the Python
# twin, against throwaway repos under $TMPDIR. Run:
#   bash -c '. scripts/lib/git-history.sh && nros_git_history_self_test'
nros_git_history_self_test() (
    set -u
    # shellcheck source=scripts/lib/git-hook-env.sh
    . "$(dirname "${BASH_SOURCE[0]}")/git-hook-env.sh"
    nros_clear_inherited_git_env
    export GIT_AUTHOR_NAME=t GIT_AUTHOR_EMAIL=t@t GIT_COMMITTER_NAME=t \
        GIT_COMMITTER_EMAIL=t@t GIT_CONFIG_NOSYSTEM=1 GIT_CONFIG_GLOBAL=/dev/null
    tmp="$(mktemp -d "${TMPDIR:-/tmp}/git-history-sh-selftest.XXXXXX")" || exit 1
    trap 'rm -rf "$tmp"' EXIT
    git init -q "$tmp/src" || exit 1
    for i in 0 1 2 3; do
        mkdir -p "$tmp/src/d/$i" && echo "$i" > "$tmp/src/d/$i/f"
        git -C "$tmp/src" add -A && git -C "$tmp/src" commit -qm "c$i" || exit 1
    done
    git -C "$tmp/src" config uploadpack.allowFilter true
    git clone -q --depth 1 "file://$tmp/src" "$tmp/shallow" </dev/null || exit 1
    git clone -q --depth 1 "file://$tmp/src" "$tmp/partial" </dev/null || exit 1
    git -C "$tmp/partial" fetch -q --no-tags --filter=tree:0 --unshallow origin </dev/null || exit 1
    bad=0
    t() { if "${@:2}"; then :; else echo "git-history.sh self-test: $1: FAILED" >&2; bad=$((bad + 1)); fi; }
    t "full: readable" eval '! nros_git_history_objects_unreadable -C "$tmp/src" >/dev/null'
    t "full: old tree here" nros_git_tree_present HEAD~2 -C "$tmp/src"
    t "shallow: unreadable" eval 'nros_git_history_objects_unreadable -C "$tmp/shallow" >/dev/null'
    t "partial: is partial" nros_git_partial -C "$tmp/partial"
    t "partial: not truncated" eval '! nros_git_history_truncated -C "$tmp/partial"'
    t "partial: unreadable" eval 'nros_git_history_objects_unreadable -C "$tmp/partial" >/dev/null'
    t "partial: tip tree here" nros_git_tree_present HEAD -C "$tmp/partial"
    t "partial: old tree absent" eval '! nros_git_tree_present HEAD~2 -C "$tmp/partial"'
    t "partial: probe fetched nothing" eval '! nros_git_tree_present HEAD~2 -C "$tmp/partial"'
    t "bounded read of HEAD" eval 'nros_git_history_run 30 -C "$tmp/partial" ls-tree -r --name-only HEAD | grep -q "^d/"'
    t "a timeout is rc 124" eval 'nros_git_history_run 0.001 -C "$tmp/src" log --name-only >/dev/null 2>&1; [ $? -eq 124 ]'
    [ "$bad" -eq 0 ] || exit 1
    echo "git-history.sh self-test: OK (full, shallow, partial)"
)
