#!/usr/bin/env bash
#
# Run the fast gate tier in a checkout with NO submodules, and require green.
#
# Issue 1373. The fast tier is documented to run green on a pristine worktree
# (issue 0650): `git worktree add` populates no submodules, and a worktree per
# task is how the parallel sessions here work. Two gates once failed hard there
# (capability-conditionals, xrce-vendored-versions). Issue 1345 turned both into
# NOT VERIFIED skips through the `nros_check_skip` ledger, which is the
# documented route (issue 1043's middle outcome). Nothing proved the fix
# COMPLETE, though. Every gate that grows a submodule dependency can bring the
# failure back, and a static census of "which gates read a submodule" cannot
# find them all. Measured: matching submodule PATHS in a gate's recipe and
# scripts selects 17 of 456 gates and misses BOTH of the original two, because
# they build the path from parts. Matching basenames selects 226, which is half
# the tier for a heuristic. So the honest check is the behavioural one: build
# the shape, run the tier, and read the verdict.
#
# The shape is a `git clone --local --shared` of HEAD. It has the same tracked
# tree as a fresh `git worktree add`, with every submodule uninitialised. Unlike
# a worktree, it registers nothing in this repository's admin dir (issue 1465:
# a gate asserts only on a repository it built). Uncommitted tracked edits and
# untracked, non-ignored files are overlaid so the run probes what you are
# EDITING, not only what you committed. Nothing outside the clone is written,
# and the clone is removed on exit.
#
# A gate that SKIPS there is fine, provided it says so: the inner `check fast`
# prints its skips in the closing `[SKIPPED]` block, and this script repeats
# them. A gate that FAILS there is the defect. Fix it by taking the ledger route
# (`nros_check_skip` / `check_skip.unverified`) with a remedy a worktree can run
# (`just setup-worktree`, or `git submodule update --init <path>`). Never fix it
# by provisioning the clone.
#
# Not a fast gate (it RUNS the fast tier, so the tier would recurse). It is in
# `.config/gate-lane-exempt.txt`, and `just ci gate` runs it after `check::fast`.
#
# Usage: scripts/ci/pristine-checkout-fast.sh [--keep]

set -uo pipefail

# Building a repository from inside a hook would otherwise write into the
# CALLER's (issues 0986/0988).
# shellcheck source=scripts/lib/git-hook-env.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/git-hook-env.sh"
nros_clear_inherited_git_env
# shellcheck source=scripts/lib/grep-q.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")/../lib" && pwd)/grep-q.sh"

keep=0
[ "${1:-}" = "--keep" ] && keep=1

repo_root="$(git rev-parse --show-toplevel)" || exit 2
cd "$repo_root" || exit 2

# Outside every checkout, so `activate.sh`'s re-rooting treats this clone as
# its OWN checkout and not a nested part of this one (issues 1280/1391).
tmp="$(mktemp -d "${TMPDIR:-/tmp}/nros-pristine.XXXXXX")" || exit 2
clone="$tmp/checkout"
cleanup() {
    if [ "$keep" = 1 ]; then
        echo "pristine-checkout: kept $clone" >&2
    else
        rm -rf "$tmp"
    fi
}
trap cleanup EXIT

if ! git clone --quiet --local --shared --no-checkout "$repo_root" "$clone" 2>"$tmp/clone.log" \
    || ! git -C "$clone" checkout --quiet --detach "$(git rev-parse HEAD)" 2>>"$tmp/clone.log"; then
    echo "pristine-checkout: could not build the clone" >&2
    sed 's/^/    /' "$tmp/clone.log" >&2
    exit 2
fi

# Overlay: tracked edits (staged + unstaged), then untracked non-ignored files.
# A gitlink change is deliberately NOT applied: the clone has no submodules.
if ! git diff --binary HEAD --ignore-submodules=all > "$tmp/edits.patch"; then
    echo "pristine-checkout: could not read the working-tree edits" >&2
    exit 2
fi
if [ -s "$tmp/edits.patch" ] \
    && ! git -C "$clone" apply --whitespace=nowarn "$tmp/edits.patch" 2>"$tmp/apply.log"; then
    echo "pristine-checkout: could not overlay the working-tree edits" >&2
    sed 's/^/    /' "$tmp/apply.log" >&2
    exit 2
fi
git ls-files -z --others --exclude-standard > "$tmp/untracked.z"
if [ -s "$tmp/untracked.z" ]; then
    xargs -0 -r cp --parents -t "$clone" < "$tmp/untracked.z"
    # Intent-to-add in the CLONE's index, so a gate that asks `git ls-files` /
    # `git grep` sees a new file, as it will once committed. Without this, a
    # new script was invisible to two such gates here and red the moment it
    # was committed.
    (cd "$clone" && xargs -0 -r git add -N -- < "$tmp/untracked.z")
fi

# The precondition this lane exists to test: no submodule is initialised.
initialised="$(git -C "$clone" submodule status 2>/dev/null | grep -vc '^-' || true)"
if [ "${initialised:-0}" != 0 ]; then
    echo "pristine-checkout: the clone has $initialised initialised submodule(s);" >&2
    echo "  it is not the shape issue 1373 is about, so its verdict proves nothing." >&2
    exit 2
fi

# The inner run is a PERSON in a fresh worktree, not this lane: a fresh shell,
# no CI identity (a gate may be stricter under GITHUB_ACTIONS, and the pin gate
# would read this job's event payload against a clone that cannot resolve it),
# and no strict-skip knob — a SKIP is the documented outcome here, a FAIL is the
# defect.
run_in_clone() {
    (
        cd "$clone" || exit 2
        unset NROS_CHECK_SKIP_STRICT NROS_SUBMODULE_PIN_BASELINE \
            GITHUB_ACTIONS CI GITHUB_EVENT_NAME GITHUB_EVENT_PATH
        # shellcheck disable=SC1091
        NROS_QUIET_ACTIVATE=1 source ./activate.sh >/dev/null 2>&1
        "$@"
    )
}

# --- selftest, on the NORMAL path -------------------------------------------
#
# The verdict below is only worth something if a gate that NEEDS a submodule
# really goes red in this clone, through the same runner. So plant one — a
# recipe whose whole job is to find content inside a submodule — run exactly that
# list, and require a non-zero exit. It proves both halves at once: the clone
# has no submodule content, and the runner's status reaches us. Then the
# planted recipe is removed byte for byte, before the real run (the clone's own
# `check-gate-lists` would otherwise see an unclassified recipe). ~3 s.
pristine_selftest() {
    local probe_file src rec
    local sub
    # The submodule is READ from .gitmodules, not spelled: which one does not
    # matter, only that its directory is empty in the clone. (A spelled path
    # is also what `check-store-source-readers` refuses for a store-first tree.)
    sub="$(git -C "$clone" config -f .gitmodules --get-regexp '^submodule\..*\.path$' \
           | awk 'NR == 1 { print $2 }')"
    if [ -z "$sub" ]; then
        echo "pristine-checkout SELFTEST FAILED: .gitmodules names no submodule" >&2
        exit 1
    fi
    probe_file="just/check/lanes.just"
    cp "$clone/$probe_file" "$tmp/lanes.just.orig"
    printf '\npristine-selftest-probe:\n    @[ -n "$(ls -A %s 2>/dev/null)" ]\n' "$sub" \
        >> "$clone/$probe_file"
    printf 'pristine-selftest-probe\n' > "$tmp/probe.list"
    src=0
    run_in_clone bash scripts/build/run-gates-parallel.sh "$tmp/probe.list" \
        > "$tmp/probe.log" 2>&1 || src=$?
    cp "$tmp/lanes.just.orig" "$clone/$probe_file"
    # Non-zero is not enough: the runner also exits non-zero when it cannot RUN
    # the list, which would pass this control while proving nothing. Require the
    # planted gate's own FAIL record.
    rec=0
    nros_grep_q '^===== FAIL (pristine-selftest-probe,' "$tmp/probe.log" || rec=$?
    if [ "$rec" -ge 2 ]; then
        echo "pristine-checkout SELFTEST: could not read $tmp/probe.log" >&2
        exit 2
    fi
    if [ "$src" -eq 0 ] || [ "$rec" -ne 0 ] \
        || ! cmp -s "$tmp/lanes.just.orig" "$clone/$probe_file"; then
        echo "pristine-checkout SELFTEST FAILED: a gate that reads a file inside an" >&2
        echo "  uninitialised submodule passed in the clone (rc=$src), so a green" >&2
        echo "  verdict from this lane would prove nothing. Its output:" >&2
        sed 's/^/    /' "$tmp/probe.log" >&2
        exit 1
    fi
}
pristine_selftest

echo "pristine-checkout: running \`just check fast\` in a submodule-free clone of"
echo "  $(git rev-parse --short HEAD) (+ working-tree edits) at $clone"
log="$tmp/fast.log"
rc=0
run_in_clone just check fast > "$log" 2>&1 || rc=$?

# The closing lines carry the verdict and the [SKIPPED] block.
sed -n '/^check-fast (parallel):/,$p' "$log"
if [ "$rc" -ne 0 ]; then
    echo "" >&2
    grep -E '^===== FAIL' "$log" >&2 || tail -40 "$log" >&2
    echo "" >&2
    echo "pristine-checkout: FAILED (rc=$rc) -- the fast tier is red in a checkout" >&2
    echo "  with no submodules, which is what \`git worktree add\` produces." >&2
    echo "  A gate whose subject lives in a submodule must report NOT VERIFIED" >&2
    echo "  through the nros_check_skip ledger, with a remedy a worktree can run" >&2
    echo "  (\`just setup-worktree\`) -- never fail (issues 1373, 1345, 1043)." >&2
    echo "  Re-run with --keep to inspect the clone." >&2
    exit 1
fi
echo "pristine-checkout: OK -- the fast tier is green with no submodules checked out."
