# shellcheck shell=bash
# Issue 2958 -- does the working tree the pre-push gates read equal the commit
# being pushed?
#
# `just check fast` judges the working tree; CI checks out the pushed commit.
# A difference between the two passes locally and fails remotely: an edit left
# unstaged (`git mv` stages the INDEX copy), a new file never added, a
# deletion never staged. This answers the question for the pre-push hook in
# milliseconds and changes no gate.
#
# Usage: nros_prepush_tree_matches <repo-root> < <hook stdin>
#   Reads git's pre-push ref lines (`<local-ref> <local-sha> <remote-ref>
#   <remote-sha>`). Only a BRANCH push whose local sha is `HEAD` is judged:
#   pushing another ref publishes a commit the working tree is not, so the
#   tree is not its subject either way. Returns 0 when nothing to judge or the
#   tree matches; prints the differing paths and returns 1 otherwise.
#
# Read-only by construction (`check-hook-repo-side-effects`): `git diff
# --quiet`, `git diff --name-status`, `git ls-files`. Submodule CONTENT is
# not judged (`--ignore-submodules=dirty`); a moved pin is.
nros_prepush_tree_matches() {
    local root="$1" head line local_sha remote_ref judged=0
    head="$(git -C "$root" rev-parse --verify --quiet HEAD)" || return 0
    while IFS= read -r line; do
        # shellcheck disable=SC2086
        set -- $line
        local_sha="${2:-}"
        remote_ref="${3:-}"
        case "$remote_ref" in refs/heads/*) ;; *) continue ;; esac
        [ "$local_sha" = "$head" ] && judged=1
    done
    [ "$judged" -eq 1 ] || return 0

    local changed untracked
    changed="$(git -C "$root" diff --name-status --ignore-submodules=dirty HEAD --)"
    untracked="$(git -C "$root" ls-files --others --exclude-standard)"
    [ -z "$changed" ] && [ -z "$untracked" ] && return 0

    {
        echo "pre-push: REFUSING to push — the working tree differs from the commit being"
        echo "          pushed, and the fast gates would judge the tree, not the push (issue 2958)."
        echo ""
        if [ -n "$changed" ]; then
            echo "  Changed against HEAD (staged or not):"
            printf '%s\n' "$changed" | sed -n '1,20s/^/    /p'
            [ "$(printf '%s\n' "$changed" | wc -l)" -gt 20 ] && echo "    ..."
        fi
        if [ -n "$untracked" ]; then
            echo "  Untracked, not ignored:"
            printf '%s\n' "$untracked" | sed -n '1,20s/^/    /p'
            [ "$(printf '%s\n' "$untracked" | wc -l)" -gt 20 ] && echo "    ..."
        fi
        echo ""
        echo "  Commit what belongs in the push, or set the rest aside and push again:"
        echo "      git stash push -u -m 'not in this push'"
        echo "  The gates then read exactly what is pushed."
        echo "  To push anyway, knowing the gates would not judge this commit:"
        echo "      NROS_SKIP_PREPUSH_CHECKS=1 git push ..."
    } >&2
    return 1
}
