# Is a submodule's CHECKOUT usable, not only its HEAD? (issue 2306)
#
# `git submodule status --cached` compares the gitlink with the submodule's
# HEAD. That is all the provisioning checks asked, and an interrupted checkout
# answers it wrongly: a submodule whose HEAD is on the pin over a working tree
# that holds only `.git` (every tracked file staged as deleted) reads as
# "initialised, on the pin". Measured on 2026-11-06 after a killed
# `just setup-worktree`: micro-cdr read as provisioned, and `just ci gate`
# failed three gates later on "vendored micro-cdr source root ... is missing".
# Three other submodules were left with HEAD on upstream `main` and their
# files staged as deleted, which `status` marks `+` and the preflight let
# through as well.
#
# This file is the one answer to "is this submodule provisioned", for the
# lane preflight (`_require-worktree-provisioning`), `just setup-worktree` and
# `check-submodule-drift.sh` (run by `check-tier-preconditions`).
#
# Usage:
#   . scripts/lib/submodule-provision.sh
#   nros_submodule_provision_state <path>   # echoes one state word, see below
#
# States, and which a caller must treat as unprovisioned:
#
#   missing          not initialised (`status` marks `-`)            RED
#   not-submodule    the path names no submodule of this checkout    RED
#   incomplete:<n>   initialised, but <n> tracked files are absent   RED
#                    from its working tree or staged as deleted: an
#                    interrupted checkout. Nothing here can build.
#   diverged         HEAD shares no line with the pin (neither is an  RED
#                    ancestor of the other): checked out at some other
#                    branch, e.g. upstream `main`.
#   behind:<n>       HEAD is <n> commits behind the pin               not red
#   ahead:<n>        HEAD is <n> commits ahead of the pin             not red
#   unknown-pin      the pin is not fetched here, or the history is   not red
#                    truncated, so the relation cannot be measured
#   ok               on the pin, working tree complete
#
# `behind` and `ahead` are not red ON PURPOSE. Ahead is a fork fix in progress
# (AGENTS.md's vendored-fork workflow). Behind is what a pull leaves until
# `git submodule update`, and `check-submodule-drift.sh` already fails it, with
# the commits listed, in `check-tier-preconditions`; failing it here too would
# refuse every lane in a checkout someone left un-updated on purpose. What
# this file adds is the two states no check could see: a broken working tree,
# and a HEAD on an unrelated line.
#
# Only the WORKING TREE's completeness is read, never its cleanliness: a
# modified file is someone's work, and `git status --porcelain` being empty
# is not a provisioning requirement.

# shellcheck source=scripts/lib/git-history.sh
. "$(dirname "${BASH_SOURCE[0]}")/git-history.sh"

nros_submodule_provision_state() {
    local p="$1" line mark recorded current deleted staged
    line="$(git submodule status --cached -- "$p" 2>/dev/null || true)"
    if [ -z "$line" ]; then
        echo not-submodule
        return 0
    fi
    mark="${line:0:1}"
    if [ "$mark" = "-" ]; then
        echo missing
        return 0
    fi
    # Tracked files the working tree lacks, and files the index has staged
    # for deletion against the submodule's own HEAD. The second is the
    # interrupted-checkout signature (an index emptied, a tree with only
    # `.git`); the first is a tree emptied under an intact index.
    deleted="$(git -C "$p" ls-files --deleted 2>/dev/null | wc -l)"
    staged="$(git -C "$p" diff --cached --name-only --diff-filter=D HEAD 2>/dev/null | wc -l)"
    if [ "$((deleted + staged))" -gt 0 ]; then
        echo "incomplete:$((deleted + staged))"
        return 0
    fi
    if [ "$mark" = " " ]; then
        echo ok
        return 0
    fi
    recorded="$(git ls-tree HEAD -- "$p" | awk '{print $3}')"
    current="$(git -C "$p" rev-parse HEAD 2>/dev/null || true)"
    if [ -z "$recorded" ] || [ -z "$current" ] \
        || ! git -C "$p" cat-file -e "${recorded}^{commit}" 2>/dev/null; then
        echo unknown-pin
        return 0
    fi
    case "$(nros_git_ancestry "$current" "$recorded" -C "$p")" in
        yes)
            echo "behind:$(git -C "$p" rev-list --count "${current}..${recorded}")"
            return 0
            ;;
        unknown)
            echo unknown-pin
            return 0
            ;;
    esac
    case "$(nros_git_ancestry "$recorded" "$current" -C "$p")" in
        yes) echo "ahead:$(git -C "$p" rev-list --count "${recorded}..${current}")" ;;
        unknown) echo unknown-pin ;;
        *) echo diverged ;;
    esac
}

# rc 0 when <state> (from `nros_submodule_provision_state`) is unprovisioned.
nros_submodule_state_is_red() {
    case "$1" in
        missing | not-submodule | incomplete:* | diverged) return 0 ;;
        *) return 1 ;;
    esac
}

# The one-line repair for a red <state> of <path>, or nothing.
nros_submodule_state_remedy() {
    local state="$1" p="$2"
    case "$state" in
        missing) echo "git submodule update --init $p" ;;
        not-submodule) echo "fix .config/worktree-provisioning.txt (renamed?)" ;;
        incomplete:*)
            echo "git submodule update --init --force $p   (an interrupted checkout; first check git -C $p status holds nothing you need)"
            ;;
        diverged)
            echo "git submodule update --force $p   (HEAD is on another line; first check it holds nothing you need)"
            ;;
    esac
}
