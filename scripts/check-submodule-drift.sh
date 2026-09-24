#!/usr/bin/env bash
# Issue 0550 — a submodule left BEHIND the pointer the superproject records.
#
# A pull advances a submodule pointer. Nothing checks it out. Hours later a
# build asks for a file that only exists in the newer commit, and the error
# names the FILE, never the submodule:
#
#   CMake Error at zephyr/cmake/modules/extensions.cmake:428 (add_library):
#     Cannot find source file:
#       third-party/dds/cyclonedds/src/ddsrt/src/sync/zephyr/sync.c
#
# That was 2026-08-13, seventeen leaves into `just build-test-fixtures`. The
# checkout sat 7 commits behind, and the commit it was missing (`a09babf`,
# ddsrt's Zephyr-native k_mutex/k_condvar sync backend) is exactly half of a
# pair CLAUDE.md already warns must move together — `DDSRT_WITH_ZEPHYR` picks
# the types, `nros_rmw_cyclonedds.cmake` swaps the TU. The in-tree half was
# current; the vendored half was not. The rule was known. What was missing is
# that nothing SAYS so, and the symptom is unrecognizable from the rule.
#
# `git submodule status | grep '^+'` is the whole check. This script only adds
# the part that makes the answer actionable: WHICH DIRECTION.
#
#   behind   (checked-out is an ancestor of recorded)  -> FAIL. `git submodule
#            update <path>` is a fast-forward, no local work at risk.
#   ahead    (recorded is an ancestor of checked-out)  -> OK, and deliberate:
#            it is the middle of CLAUDE.md's vendored-fork workflow, where the
#            agent commits + rebases locally and the maintainer pushes before
#            the superproject pointer is bumped. Failing here would flag the
#            correct state of every in-flight fork fix.
#   diverged (neither)                                 -> FAIL, and it needs a
#            rebase, not an update — `git submodule update` would DISCARD the
#            local commits by checking out the recorded one detached.
#
# "neither" is only a measurement where the history is COMPLETE. A shallow
# submodule clone grafts its tip parentless, so `merge-base --is-ancestor`
# answers no in both directions for a pair that is perfectly related, and this
# would then tell the reader to rebase work that needs no rebasing. A truncated
# store reports NOT VERIFIED instead — `scripts/lib/git-history.sh` holds the
# rule, issue 1476 is what it cost the other time it was written by hand.
#
# UNINITIALIZED submodules ('-' prefix) are not drift and are not reported:
# px4, play_launch's layer-3 runtime submodules and the nuttx tree are all
# deliberately absent until a recipe inits them.
#
# Not in `check-fast`, on purpose. Drift is a WORKING-COPY state: the index and
# the commit always agree in anything you can push, so this can never fail in
# CI. It is a tier PRECONDITION, which is why it hangs off
# check-tier-preconditions.sh instead.
set -uo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [ "${NROS_SKIP_SUBMODULE_DRIFT_CHECK:-0}" != "0" ]; then
    exit 0
fi

# shellcheck source=scripts/lib/git-history.sh
. "$repo_root/scripts/lib/git-history.sh"

fail=0
ahead=0
unverified=0

# `git submodule status` marks a mismatch between the checked-out commit and
# the one in the index with a leading '+'; 'U' is an unmerged conflict. Both
# want a human. NOT --recursive: layer-3 submodules under play_launch are never
# initialized here (phase-332), and descending would report them as absent.
while IFS= read -r line; do
    [ -n "$line" ] || continue
    mark="${line:0:1}"
    rest="${line:1}"
    current="${rest%% *}"
    path="$(printf '%s' "${rest#* }" | awk '{print $1}')"
    [ -n "$path" ] || continue

    if [ "$mark" = "U" ]; then
        echo "  [x] $path has an UNMERGED submodule conflict" >&2
        echo "      resolve it in the superproject before building" >&2
        fail=$((fail + 1))
        continue
    fi

    recorded="$(git ls-tree HEAD -- "$path" | awk '{print $3}')"
    if [ -z "$recorded" ]; then
        continue   # not tracked at HEAD (added/removed in the worktree)
    fi

    # The recorded commit can be absent locally when the pointer moved in a
    # pull that did not fetch the submodule. That is still 'behind' — and
    # `git submodule update` fetches, so the remedy is the same one.
    if ! git -C "$path" cat-file -e "${recorded}^{commit}" 2>/dev/null; then
        echo "  [x] $path is at ${current:0:9}; HEAD records ${recorded:0:9}, which is not fetched" >&2
        echo "      remedy: git submodule update $path" >&2
        fail=$((fail + 1))
        continue
    fi

    if [ "$(nros_git_ancestry "$current" "$recorded" -C "$path")" = yes ]; then
        behind="$(git -C "$path" rev-list --count "${current}..${recorded}")"
        echo "  [x] $path is $behind commit(s) BEHIND the recorded pointer" >&2
        echo "      at ${current:0:9}, HEAD records ${recorded:0:9}" >&2
        git -C "$path" log --oneline "${current}..${recorded}" | sed 's/^/        /' >&2
        echo "      remedy: git submodule update $path   (fast-forward, no local work at risk)" >&2
        fail=$((fail + 1))
    elif [ "$(nros_git_ancestry "$recorded" "$current" -C "$path")" = yes ]; then
        # Local work ahead of the pointer — the vendored-fork workflow's normal
        # middle state. Say it, do not fail it.
        ahead=$((ahead + 1))
        echo "check-submodule-drift: note — $path is AHEAD of the recorded pointer" >&2
        echo "  ($(git -C "$path" rev-list --count "${recorded}..${current}") local commit(s)). Expected mid-fork-fix; push the fork" >&2
        echo "  branch, then bump the superproject pointer to the pushed commit." >&2
    elif nros_git_history_truncated -C "$path"; then
        # NOT a verdict. The graft, not the pins, is a sufficient explanation
        # for "no ancestry either way" — and telling someone to rebase work
        # that is already on the line is worse than saying nothing.
        unverified=$((unverified + 1))
        echo "check-submodule-drift: NOT VERIFIED — $path" >&2
        echo "  at ${current:0:9}, HEAD records ${recorded:0:9}; this clone is SHALLOW," >&2
        echo "  so neither direction of ancestry could be measured. Deepen it to get a" >&2
        echo "  verdict: git -C $path fetch --unshallow" >&2
    else
        echo "  [x] $path has DIVERGED from the recorded pointer" >&2
        echo "      at ${current:0:9}, HEAD records ${recorded:0:9} — no ancestry either way" >&2
        echo "      remedy: rebase the local commits onto ${recorded:0:9} inside $path." >&2
        echo "              do NOT 'git submodule update' — it checks out the recorded" >&2
        echo "              commit detached and leaves your commits unreferenced." >&2
        fail=$((fail + 1))
    fi
done < <(git submodule status 2>/dev/null | grep -E '^[+U]')

if [ "$fail" -ne 0 ]; then
    cat >&2 <<'EOF'

A submodule's checkout does not match the commit this superproject records.
The build reads the WORKTREE, so it compiles against whatever is on disk — and
when the two halves of a vendored/in-tree pair disagree, the failure surfaces
as a missing file or a layout mismatch that names neither the submodule nor
the pull that moved it.

Bypass: NROS_SKIP_SUBMODULE_DRIFT_CHECK=1
EOF
    exit 1
fi

narrowing=""
if [ "$unverified" -ne 0 ]; then
    # The verdict line states the NARROWING, not just the outcome — the
    # `submodule-pins` precedent. An OK that skipped a pin claims a check that
    # did not happen.
    narrowing="; $unverified NOT VERIFIED (shallow submodule clone)"
fi
if [ "$ahead" -eq 0 ]; then
    echo "check-submodule-drift: OK (every initialized submodule matches its recorded pointer)$narrowing"
else
    echo "check-submodule-drift: OK ($ahead submodule(s) ahead of the pointer — see note above)$narrowing"
fi
