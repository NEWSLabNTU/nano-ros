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
