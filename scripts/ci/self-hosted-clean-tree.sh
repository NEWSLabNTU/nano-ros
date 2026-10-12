#!/usr/bin/env bash
#
# Start a self-hosted gate job from a CLEAN tree, and prove it. phase-489 W2.
#
# usage: scripts/ci/self-hosted-clean-tree.sh [--check] [<worktree>]
#
#   (default)  clean, then assert
#   --check    assert only; exit 1 and list what is left
#
# WHY THIS EXISTS
#
# A hosted runner is a fresh VM: whatever a job reads, that job made. A
# self-hosted runner here is not. Its checkout lives in the `nros-runner-work`
# volume, which outlives every ephemeral runner container, and is shared by
# every workflow that lands on that machine (queue L3, the nightly matrix,
# run-matrix, and now the gate siblings). `actions/checkout` cleans only half of
# it: `git clean -ffdx` + `git reset --hard` in the SUPERPROJECT. A submodule's
# worktree is a gitlink as far as that clean is concerned, so an earlier job's
# submodule checkout — at an earlier job's pin, with its build output inside it —
# survives untouched. The provisioning verbs then see it and say
# `[already present (skip)]`.
#
# That is issue 0196's trap with issue 1476's mechanism: a gate that passes
# because of what an EARLIER run left, not because of what this run built. The
# user's rule for CI is the opposite (prereq -> build -> test, every run, no
# leftovers standing in for a step), so a moved job must start where a hosted
# job starts.
#
# WHAT IS KEPT, deliberately, and why each is safe:
#
#   ~/.cargo/registry, ~/.cargo/git   content-addressed downloads (checksums in
#                                     Cargo.lock); a stale entry cannot satisfy
#                                     a different version
#   ~/.cache/sccache                  content-hashed compiler outputs; a hit is
#                                     by input hash, never by mtime — the reason
#                                     the hosted jobs cache sccache and not
#                                     `target/` (gate.yml's sccache comment)
#   ~/.rustup, ~/.nros store          pinned prerequisites, stored per version;
#                                     each job's own `just setup-*` / `nros
#                                     setup` verb re-asserts them every run
#   .git/objects, .git/modules        object stores — content-addressed. NOT
#                                     evidence for an ancestry NEGATIVE (issue
#                                     1476), and no moved job asks one
#
# Everything else in the worktree goes: untracked AND ignored files (`target/`,
# `target-*/`, `build/`, generated trees, fixture stamps), and every submodule
# worktree. The job's own steps then re-initialise exactly the submodules it
# needs, at THIS commit's pins.
#
# A persistent target dir OUTSIDE the tree would defeat all of the above, so
# `CARGO_TARGET_DIR` and a `build.target-dir` in the runner's cargo config are
# refused rather than tolerated.
set -euo pipefail

mode=clean
if [ "${1:-}" = "--check" ]; then
    mode=check
    shift
fi
tree="${1:-${GITHUB_WORKSPACE:-$PWD}}"
cd "$tree"

fail=0
say() { printf 'self-hosted-clean-tree: %s\n' "$*"; }

# --- a target dir that lives outside the tree ---------------------------------
if [ -n "${CARGO_TARGET_DIR:-}" ]; then
    say "FAIL: CARGO_TARGET_DIR=$CARGO_TARGET_DIR is set in the runner's environment."
    say "  A target dir outside the worktree survives every clean below, so a"
    say "  build could read an earlier job's artifacts. Remove it from the image."
    fail=1
fi
cargo_home="${CARGO_HOME:-$HOME/.cargo}"
# shellcheck source=scripts/lib/grep-q.sh
. "$(dirname "$0")/../lib/grep-q.sh"
for cfg in "$cargo_home/config.toml" "$cargo_home/config"; do
    [ -f "$cfg" ] || continue
    rc=0
    nros_grep_q -E '^[[:space:]]*target-dir[[:space:]]*=' "$cfg" || rc=$?
    case "$rc" in
        0) say "FAIL: $cfg sets build.target-dir — same hole as CARGO_TARGET_DIR."
           fail=1 ;;
        1) ;;
        *) say "FAIL: could not read $cfg (grep rc=$rc)"
           fail=1 ;;
    esac
done

# --- clean --------------------------------------------------------------------
if [ "$mode" = clean ]; then
    # Say what an earlier job left, so every run's log is evidence that the
    # clean had something to do (or did not) rather than a bare "ok".
    subs="$(git submodule status 2>/dev/null | grep -c -v '^-' || true)"
    stale="$(git status --porcelain=v1 --ignored=matching --untracked-files=normal)"
    say "found: ${subs:-0} submodule worktree(s), $(printf '%s' "$stale" | grep -c . || true) untracked/ignored top-level entr(y/ies)"
    printf '%s\n' "$stale" | grep . | head -20 | sed 's/^/    /' || true
    # Every submodule worktree, including ones this job will never touch: a
    # leftover one is exactly what a provisioning verb mistakes for "present".
    # `deinit --force` empties the worktree and drops the registration; the
    # object store in .git/modules stays, so re-initialising costs a checkout,
    # not a clone.
    # Only the initialised ones: `deinit --all` also visits every never-
    # initialised entry and warns once per submodule about a config it lacks.
    git submodule status 2>/dev/null | awk '$0 !~ /^-/ { print $2 }' \
        | while IFS= read -r sub; do
            git submodule --quiet deinit --force -- "$sub"
          done
    # A submodule directory that is no longer in .gitmodules (a moved or retired
    # one) is invisible to `deinit`, and is an embedded repo to `clean`, which
    # needs `-ff` to remove it.
    git clean -ffdxq
    git reset --quiet --hard HEAD
fi

# --- assert -------------------------------------------------------------------
# The proof, every run: nothing untracked, nothing ignored, no submodule
# checked out. A clean that silently missed something is the failure this
# script exists for, so the check is not optional and not a warning.
left="$(git status --porcelain=v1 --ignored=matching --untracked-files=all)"
if [ -n "$left" ]; then
    say "FAIL: the worktree still holds state this job did not make:"
    printf '%s\n' "$left" | head -40 | sed 's/^/    /'
    n="$(printf '%s\n' "$left" | grep -c .)"
    [ "$n" -le 40 ] || say "    ... and $((n - 40)) more"
    fail=1
fi
live="$(git submodule status 2>/dev/null | grep -v '^-' || true)"
if [ -n "$live" ]; then
    say "FAIL: submodule worktree(s) still checked out:"
    printf '%s\n' "$live" | sed 's/^/    /'
    fail=1
fi

if [ "$fail" -ne 0 ]; then
    exit 1
fi
say "ok — $(git rev-parse --short=12 HEAD), no untracked/ignored files, no submodule worktree"
