# shellcheck shell=sh
# The probe tests ONE commit, named by SHA (phase-567 W3c).
#
# Sourced by run-bootstrap-probe.sh, by build-release-asset.sh inside the
# release builder, and prepended to the extracted book script (--prelude), so
# the user container clones the same way. POSIX sh: probe.sh may run under
# bash or zsh (PROBE_SHELL).
#
# Why a SHA and never a branch name: the release workflow checks out the
# commit it releases by SHA, which leaves a DETACHED HEAD and no local `main`,
# and the probe was handed `PROBE_BRANCH=main` from `github.ref_name`. So
# `git show main:…` failed on the first rehearsal (`fatal: invalid object
# name 'main'`), and where `main` did exist a `git clone --branch main` built
# main's TIP, not the commit under test. The checked-out commit is the only
# thing every caller agrees on.

# probe_clone_at_commit <url> <sha> <dest> [depth]
# Fetch exactly <sha> from <url> into a new repo at <dest> and check it out
# DETACHED (actions/checkout's shape). A fetch by object id needs no ref to
# name it: protocol v2 serves any object the source has, which is what lets a
# linked worktree's HEAD (not advertised by the shared git dir) be cloned.
probe_clone_at_commit() (
    # A subshell body: nothing it sets leaks, and `exit` ends only it.
    # Issue 0986 — an inherited GIT_DIR would turn the `git init` below into a
    # write to the CALLER's repository. On the host the shared helper is loaded
    # (run-bootstrap-probe.sh and check-book-probe-tracks.py source
    # scripts/lib/git-hook-env.sh first); a probe container inherits no
    # repository environment and has no copy of it.
    if command -v nros_clear_inherited_git_env >/dev/null 2>&1; then
        nros_clear_inherited_git_env
    fi
    _pcc_url=$1
    _pcc_sha=$2
    _pcc_dest=$3
    _pcc_depth=${4:-}
    git init -q "$_pcc_dest" || exit 1
    git -C "$_pcc_dest" remote add origin "$_pcc_url" || exit 1
    if [ -n "$_pcc_depth" ]; then
        git -C "$_pcc_dest" fetch -q --depth "$_pcc_depth" origin "$_pcc_sha" || exit 1
    else
        # --update-shallow: the source is itself shallow wherever CI made it
        # (actions/checkout is depth 1), and without it git REJECTS the
        # shallow root with a warning, exits 0, and fetches nothing.
        git -C "$_pcc_dest" fetch -q --update-shallow origin "$_pcc_sha" || exit 1
    fi
    git -C "$_pcc_dest" checkout -q --detach FETCH_HEAD || exit 1
    _pcc_got=$(git -C "$_pcc_dest" rev-parse HEAD) || exit 1
    if [ "$_pcc_got" != "$_pcc_sha" ]; then
        echo "probe: clone of $_pcc_sha landed on $_pcc_got" >&2
        exit 1
    fi
)
