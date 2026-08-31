#!/usr/bin/env bash
#
# Keep an EPHEMERAL runner available: register, take one job, sweep, repeat.
#
#   scripts/ci/runner-loop.sh <labels> [--once] [--max N]
#
# WHY THIS EXISTS — it is not a convenience wrapper.
#
# `--ephemeral` is the right security trade (one job cannot leave state for the
# next), but it means the runner DE-REGISTERS AND EXITS after every job. On its
# own that is merely inconvenient. Combined with a REQUIRED status check that
# only a self-hosted runner can satisfy — `L3 (cross build + link)` — it is a
# merge-queue deadlock: the next entry waits for a verdict from a runner that no
# longer exists, and a check that never reports blocks forever rather than
# failing. It does not look like breakage; it looks like GitHub being slow.
#
# `runner-register.sh` says something must re-register, and lists this as the
# preferred option. This is that something.
#
# Between jobs it runs the sweep: one leaked DDS peer becomes every later job's
# flake (this repo has found 71 orphaned servers at once), and a full disk fails
# in ways that read as code failures.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
RUNNER_DIR="${NROS_RUNNER_DIR:-$HOME/actions-runner}"
LABELS="" ONCE=0 MAX=0 N=0

while [ $# -gt 0 ]; do
    case "$1" in
        --once)  ONCE=1 ;;
        --max)   MAX="${2:?--max needs a count}"; shift ;;
        -h|--help) sed -n '2,25p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*) echo "runner-loop: unknown option '$1'" >&2; exit 2 ;;
        *)  LABELS="$1" ;;
    esac
    shift
done
[ -n "$LABELS" ] || { echo "runner-loop: need <labels>" >&2; exit 2; }

cleanup() { echo "runner-loop: stopping (took $N job(s))"; exit 0; }
trap cleanup INT TERM

echo "runner-loop: labels=$LABELS dir=$RUNNER_DIR"
echo "  Ctrl-C to stop. Each iteration = one job, then a sweep."

while :; do
    # Re-register when the previous job spent the registration. `--replace`
    # makes this idempotent, so a still-valid registration costs one API call
    # rather than an error.
    if [ ! -f "$RUNNER_DIR/.credentials" ]; then
        echo "runner-loop: registering (iteration $((N + 1)))"
        "$REPO_ROOT/scripts/ci/runner-register.sh" "$LABELS" >/dev/null || {
            echo "runner-loop: registration FAILED — not retrying blind." >&2
            echo "  A retry loop against a failing registration is a way to" >&2
            echo "  hammer the API and hide the reason. Re-run by hand:" >&2
            echo "      just runner-register $LABELS" >&2
            exit 1; }
    fi

    # run.sh returns when the job finishes and the ephemeral runner retires.
    ( cd "$RUNNER_DIR" && ./run.sh ) || true
    N=$((N + 1))
    echo "runner-loop: job $N finished; sweeping"
    "$REPO_ROOT/scripts/ci/runner-sweep.sh" || echo "  (sweep reported a problem — continuing)"

    [ "$ONCE" -eq 1 ] && cleanup
    [ "$MAX" -gt 0 ] && [ "$N" -ge "$MAX" ] && cleanup
done
