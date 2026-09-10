#!/usr/bin/env bash
#
# Keep an EPHEMERAL CONTAINED runner available: fresh container, one job, repeat.
#
#   scripts/ci/runner-loop-container.sh <labels> [--once] [--max N] [--check]
#
# This is `runner-loop.sh` for the contained path, and the two differ in exactly
# one way that matters: the bare-host loop re-registers a runner that stays
# installed, while this one starts a NEW CONTAINER per job. Everything a job
# wrote to the container's writable layer is gone before the next one starts,
# which is the property that stops the orphan and disk rot a persistent runner
# accumulates (71 orphaned processes, oldest ten days, measured on ours).
#
# WHY A LOOP AT ALL. `--ephemeral` retires the runner after one job, and `L3
# (cross build + link)` is a REQUIRED check that only a self-hosted runner
# satisfies. With no runner to take the next entry, the merge queue does not
# fail — it WAITS, forever, which looks like GitHub being slow.
#
# WHAT PERSISTS ACROSS ITERATIONS is only what is in a named volume: the SDK
# store, the cargo and sccache caches, `_work`, and the nano-ros checkout the
# label gate reads. Those are populated ONCE by `runner-bootstrap.sh`, outside
# this loop, because provisioning per job would pay for it per job.
#
# THE EXIT CODES ARE THE DESIGN. A container exiting is ambiguous on its own —
# it finished a job, or it could never have worked. The entrypoint distinguishes
# them: 78 (EX_CONFIG) means the labels are not true here, and no number of
# restarts will make them true. Restarting on 78 is a storm against whatever is
# broken; stopping on 0 leaves the queue hanging. So:
#
#     0   job done            -> sweep, start a fresh container
#     78  labels not true     -> STOP and say what to run
#     *   run.sh failed       -> back off, and stop after 3 IN A ROW
#
# The third case is a compromise with evidence on both sides: a single failure
# is usually a transient registration or network error and retrying is right; a
# permanent one retried forever is the storm again. Three consecutive failures
# is the line, and the container runs in the FOREGROUND, so its own output is
# already on this loop's stdout — there is no dead container to fish logs out of
# afterwards, which is the other half of why `--attach` exists.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
ENGINE="${NROS_CONTAINER_ENGINE:-docker}"
NAME="${NROS_RUNNER_NAME:-nano-ros-runner}"
LABELS="" ONCE=0 MAX=0 CHECK=0 N=0 FAILS=0
MAX_CONSECUTIVE_FAILURES=3

while [ $# -gt 0 ]; do
    case "$1" in
        --once)  ONCE=1 ;;
        --max)   MAX="${2:?--max needs a count}"; shift ;;
        --check|--dry-run) CHECK=1 ;;
        -h|--help) sed -n '2,40p' "$0" | sed 's/^# \{0,1\}//'; exit 0 ;;
        -*) echo "runner-loop-container: unknown option '$1'" >&2; exit 2 ;;
        *)  LABELS="$1" ;;
    esac
    shift
done
[ -n "$LABELS" ] || { echo "runner-loop-container: need <labels>" >&2; exit 2; }
LABELS="${LABELS// /,}"

say() { echo "runner-loop-container: $*"; }

# The repo, resolved the way `runner-up.sh` resolves it: from the ORIGIN remote,
# never a hardcoded default, so a fork's operator does not silently attach a
# runner to somebody else's repo.
REPO="${GH_REPO:-}"
if [ -z "$REPO" ] && command -v gh >/dev/null 2>&1; then
    REPO="$(gh repo view --json nameWithOwner -q .nameWithOwner 2>/dev/null || true)"
fi
[ -n "$REPO" ] || { echo "runner-loop-container: cannot determine the repo — set GH_REPO." >&2; exit 2; }

# A registration token lives ~1 hour and is spent by one `--ephemeral` runner,
# so it is minted PER ITERATION rather than once. Minting up front and reusing
# it is what makes a loop die overnight with an authentication error six hours
# after the operator stopped watching.
mint_token() {
    gh api -X POST "repos/${REPO}/actions/runners/registration-token" --jq .token 2>/dev/null
}

if [ "$CHECK" -eq 1 ]; then
    say "repo=$REPO labels=$LABELS"
    echo "  would, per iteration:"
    echo "    mint a registration token (gh api, needs admin on $REPO)"
    echo "    $ENGINE rm -f $NAME  (only if a stale container of that name exists)"
    echo "    runner-container.sh $LABELS --run --attach"
    echo "    runner-sweep.sh"
    echo "  stops on exit 78 (labels not true) or $MAX_CONSECUTIVE_FAILURES failures in a row"
    exit 0
fi

# A store that was never bootstrapped produces exit 78 on the FIRST iteration,
# which is correct but wastes a token and a registration to say so. Ask here,
# where the answer is free.
if ! "$ENGINE" volume inspect nros-runner-src >/dev/null 2>&1; then
    echo "runner-loop-container: the checkout volume 'nros-runner-src' does not exist." >&2
    echo "  The label gate has nothing to read, so every container would exit 78." >&2
    echo "  Provision the store once, then start this loop:" >&2
    echo "      just runner-bootstrap $LABELS" >&2
    exit 2
fi

stop() { say "stopping after $N job(s)"; exit "${1:-0}"; }
trap 'stop 0' INT TERM

say "repo=$REPO labels=$LABELS  (Ctrl-C to stop; each iteration is one job)"

while :; do
    # A container of this name left behind by a previous run — a `-d` start, or
    # a kill that outran `--rm` — makes `docker run` fail on the NAME, which
    # reads like a runner problem and is not one.
    "$ENGINE" rm -f "$NAME" >/dev/null 2>&1 || true

    TOKEN="$(mint_token || true)"
    [ -n "$TOKEN" ] || {
        echo "runner-loop-container: could not mint a registration token for $REPO." >&2
        echo "  This needs ADMIN on the repo; an authenticated gh is not enough." >&2
        exit 1; }

    say "starting container for iteration $((N + 1))"
    RC=0
    GH_REPO="$REPO" RUNNER_TOKEN="$TOKEN" \
        "$REPO_ROOT/scripts/ci/runner-container.sh" "$LABELS" --run --attach || RC=$?

    case "$RC" in
        0)
            N=$((N + 1)); FAILS=0
            say "job $N finished; sweeping"
            "$REPO_ROOT/scripts/ci/runner-sweep.sh" || say "(sweep reported a problem — continuing)"
            ;;
        78)
            echo "runner-loop-container: the container refused to register — its" >&2
            echo "  labels are not true in the store it was given. Restarting" >&2
            echo "  cannot change that, so this loop stops here." >&2
            echo "      just runner-bootstrap $LABELS" >&2
            exit 78
            ;;
        *)
            FAILS=$((FAILS + 1))
            say "container exited $RC (failure $FAILS of $MAX_CONSECUTIVE_FAILURES in a row)"
            if [ "$FAILS" -ge "$MAX_CONSECUTIVE_FAILURES" ]; then
                echo "runner-loop-container: $FAILS consecutive failures — stopping" >&2
                echo "  rather than hammering whatever is broken. The container ran" >&2
                echo "  in the foreground, so its output is directly above this." >&2
                exit "$RC"
            fi
            sleep $((FAILS * 15))
            ;;
    esac

    [ "$ONCE" -eq 1 ] && stop 0
    [ "$MAX" -gt 0 ] && [ "$N" -ge "$MAX" ] && stop 0
done
