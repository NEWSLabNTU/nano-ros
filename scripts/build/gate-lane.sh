#!/usr/bin/env bash
# Run one gate LANE through the nextest harness (phase-543 W5).
#
#   scripts/build/gate-lane.sh <lane> [nextest args...]
#
# `<lane>` is a `[lane.*]` of `.config/gate-lanes.toml`: its tiers and claims
# decide which gates are in scope; the rest are listed as ignored, by name.
# This is the old `run-gates-parallel.sh`'s pre- and post-steps around
# `just check nextest-gates`, with the scheduling left to nextest:
#
# before
#   * refuse a TMPDIR inside a git checkout, and bound git discovery (issue 2240);
#   * the skip-ledger self-test (a skip that cannot be recorded cannot fail);
#   * clear the GITHUB_* step files, so no gate writes this job's outputs;
#   * complete the pinned Rust toolchains, as [INFRA] (issue 1868);
#   * the lane's claims are present (`gate-lane-preflight.py`), all named at once;
#   * one CPU budget: each gate's own nextest gets `NROS_GATE_CPU_SHARE` threads;
#   * ONE token pool for the whole lane (phase-489 W13, `nros_pool_own`):
#     `NROS_GATE_TOKENS` (default nproc) make 4.4 FIFO jobserver tokens, which
#     every cargo, make and ninja below every gate draws from. Without a
#     pinned make 4.4 the lane says the pool is OFF and runs as before.
# after
#   * the load report: host runnable p50/p90/max against the pool, and each
#     gate whose processes peaked above its declared weight, by name
#     (`scripts/build/run-load-sampler.py`); a child outside the pool is
#     visible there, not hidden. Only this run's processes count: each run
#     exports its own `NROS_GATE_RUN` id, since the lane NAME is shared by
#     every worktree (issue 2683). `NROS_GATE_WAITS=1` adds what each gate's
#     threads WAITED on (token, child, file lock, disk), sampled every 5 s;
#   * no TRACKED file changed while the gates ran (issue 2186), on every lane;
#   * no UNTRACKED, unignored file appeared. A gate's output belongs under an
#     ignored root (`build/`, `target/`, `tmp/`); phase-543 W4 found 1.1 G of
#     probe cache written into `packages/api/nros-c/` that nothing saw.
#
# The lane runs under `nros_guard_lane`, so stopping it stops every gate below.
set -uo pipefail
cd "$(dirname "$0")/../.."

lane="${1:-}"
if [ -z "$lane" ]; then
    echo "usage: $0 <lane> [nextest args...]   (lanes: .config/gate-lanes.toml)" >&2
    exit 2
fi
shift

# shellcheck source=scripts/lib/git-hook-env.sh
. scripts/lib/git-hook-env.sh
if tmp_top="$(nros_clear_inherited_git_env; unset GIT_CEILING_DIRECTORIES
        git -C "${TMPDIR:-/tmp}" rev-parse --show-toplevel 2>/dev/null)"; then
    echo "$0: REFUSING to run: TMPDIR=${TMPDIR:-/tmp} is inside the git checkout $tmp_top." >&2
    echo "  Gates' self-tests build fixtures that must be outside every checkout," >&2
    echo "  and a scratch repository there can reach that checkout (issue 2240)." >&2
    exit 2
fi
nros_bound_git_discovery

if ! bash -c '. scripts/build/check-skip.sh && nros_check_unverified_self_test' \
        || ! python3 scripts/lib/check_skip.py >/dev/null 2>&1; then
    echo "$0: the check-skip ledger SELF-TEST FAILED — a skip would not be recorded; refusing to run" >&2
    exit 2
fi

unset GITHUB_ENV GITHUB_OUTPUT GITHUB_PATH GITHUB_STATE GITHUB_STEP_SUMMARY

if ! scripts/lib/install-rust-toolchains.sh; then
    echo "$0: [INFRA] could not complete the pinned Rust toolchains; no gate ran (issue 1868)." >&2
    exit 1
fi

if ! python3 scripts/check/gate-lane-preflight.py "$lane"; then
    exit 1
fi

# phase-584 W3 (RFC-0131 D3/D5) -- the lane's preconditions: gates every other
# gate reads the subject of (the CLI's and the resolver's freshness), run here,
# serially and fail-fast, BEFORE nextest starts, so none of them races a gate
# that needs it. Each verdict is written to a directory of THIS run, which the
# harness reads for its `precondition::check::<name>` test: a precondition is
# in the junit by name and runs once. A stale verdict cannot pass a later run,
# because the directory is new.
if ! preconditions="$(python3 scripts/check/check-gate-lists.py --preconditions "$lane")"; then
    exit 1
fi
NROS_GATE_PRECONDITIONS="$(mktemp -d "${TMPDIR:-/tmp}/nros-gate-pre.XXXXXX")"
export NROS_GATE_PRECONDITIONS
for p in $preconditions; do
    echo "gate lane \`$lane\`: precondition \`check::$p\`"
    p_rc=0
    just check "$p" >"$NROS_GATE_PRECONDITIONS/$p.log" 2>&1 || p_rc=$?
    echo "$p_rc" >"$NROS_GATE_PRECONDITIONS/$p.rc"
    if [ "$p_rc" -ne 0 ]; then
        cat "$NROS_GATE_PRECONDITIONS/$p.log" >&2
        echo "gate lane \`$lane\`: precondition \`check::$p\` FAILED (rc=$p_rc); no gate ran." >&2
        rm -rf "$NROS_GATE_PRECONDITIONS"
        exit 1
    fi
done

jobs="${NROS_GATE_JOBS:-$(nproc)}"
_share=$(( $(nproc 2>/dev/null || echo 4) / jobs ))
[ "$_share" -lt 1 ] && _share=1
export NROS_GATE_CPU_SHARE="${NROS_GATE_CPU_SHARE:-$_share}"
export NROS_GATE_JOBS="$jobs" NROS_GATE_LANE="$lane" GIT_OPTIONAL_LOCKS=0
# Issue 2683 -- this run's own id, so the load sampler counts this lane's
# processes and not another worktree's lane of the same name.
export NROS_GATE_RUN="${lane}-$$-$(date +%s)"

out_dir="$(mktemp -d "${TMPDIR:-/tmp}/nros-gate-lane.XXXXXX")"
trap 'rm -rf "$out_dir" "${NROS_GATE_PRECONDITIONS:-}"' EXIT

# Tracked: a content hash of each file that differs from HEAD, so an edit that
# was already there before the run is not blamed on a gate. Untracked: the
# unignored set, by name.
_snapshot() {
    git diff --name-only --no-renames --ignore-submodules=all HEAD -- 2>/dev/null \
        | while IFS= read -r f; do
            if [ -e "$f" ]; then
                printf 'tracked %s %s\n' "$(git hash-object -- "$f")" "$f"
            else
                printf 'tracked deleted %s\n' "$f"
            fi
        done
    git ls-files --others --exclude-standard -- 2>/dev/null | sed 's/^/untracked - /'
}
_snapshot | sort >"$out_dir/before"

# shellcheck source=scripts/build/subtree-guard.sh
. scripts/build/subtree-guard.sh
# shellcheck source=scripts/build/jobserver-pool.sh
. scripts/build/jobserver-pool.sh
tokens="${NROS_GATE_TOKENS:-$(nproc 2>/dev/null || echo 4)}"
sample_interval=30
[ "${NROS_GATE_WAITS:-}" = 1 ] && sample_interval=5
python3 scripts/build/run-load-sampler.py sample "$out_dir/load" --interval "$sample_interval" \
    --pool "$tokens" &
sampler=$!
rc=0
nros_guard_lane "gates-$lane" nros_pool_own "gate lane \`$lane\`" "$tokens" \
    just check nextest-gates "$@" || rc=$?
kill "$sampler" 2>/dev/null
wait "$sampler" 2>/dev/null
python3 scripts/build/run-load-sampler.py report "$out_dir/load" "$tokens" || true
if [ "${NROS_GATE_WAITS:-}" = 1 ]; then
    # The raw samples outlive the lane's temp dir, for a closer look.
    keep="build/gates/waits/$NROS_GATE_RUN"
    mkdir -p "$keep" && cp "$out_dir"/load* "$keep"/ 2>/dev/null \
        && echo "waits: raw samples kept in $keep"
fi

_snapshot | sort >"$out_dir/after"
changed="$(diff "$out_dir/before" "$out_dir/after" | sed -n 's/^[<>] //p' \
    | awk '{print $1, $3}' | sort -u)"
if [ -n "$changed" ]; then
    printf '\ngate lane `%s`: FAILED — the source tree changed while the gates ran:\n' "$lane" >&2
    printf '%s\n' "$changed" | sed 's/^/  /' >&2
    echo "  A gate compares against a temp copy and writes its output under an ignored" >&2
    echo "  root (build/, target/, tmp/), never into the tree: a write there dirties it," >&2
    echo "  re-stales fixtures, and other gates read text nobody committed (issue 2186)." >&2
    echo "  If YOU edited one of these during the run, re-run the lane." >&2
    [ "$rc" -eq 0 ] && rc=1
fi
exit "$rc"
