#!/usr/bin/env bash
#
# Run a CI lane's steps IN ORDER, timed, and say where the time went (issue 1500).
#
# Usage:  scripts/ci/run-lane-steps.sh <lane> <step>...
#
# Each <step> is a `just` recipe path (`check::fast`, `check::default`,
# `test-all`). The lane STOPS at the first failing step — the trade every lane in
# `just/ci.just` makes for feedback — and the table at the end names what that
# withdrew, so a red count is never read as a coverage report (issue 0952).
#
# WHY THIS EXISTS. `host-tests` ran `just ci tier1` for 75 minutes on
# 2026-10-04 (run 37176625915) and the ceiling killed it. Its 5 579-line log had
# no timestamp anywhere, so "where did the time go" was answerable only by
# reading cargo's own `Finished … in` lines and the gate runner's busy/wall
# summaries, and the step the ceiling killed was a guess. Every step now prints
# a start line and an end line carrying its wall clock, AS IT FINISHES — a lane
# killed mid-step still leaves every completed step's duration in the job log,
# which is the one channel a timeout does not take with it.
#
# One runner for every lane that sequences steps (`gate`, `tier1`, the tier-2
# runs, `full`), rather than a copy of the loop in each: four hand-written loops
# had already drifted into three output shapes, and only two of them reported
# what a failure withdrew.
#
# Markers start with `==>` / `<==` so a workflow can stream just these lines to
# its live log while the full output goes to a file.
set -uo pipefail

if [ "$#" -lt 2 ]; then
    echo "usage: $0 <lane> <step>..." >&2
    exit 2
fi

lane="$1"
shift
steps=("$@")
n="${#steps[@]}"

fmt_dur() {
    local s="$1"
    if [ "$s" -ge 3600 ]; then
        printf '%dh%02dm%02ds' $((s / 3600)) $((s % 3600 / 60)) $((s % 60))
    else
        printf '%dm%02ds' $((s / 60)) $((s % 60))
    fi
}

now_utc() { date -u '+%H:%M:%SZ'; }

# issue 1754 — the markers are also the POSITIVE record of how far the lane got.
# A workflow's cells step is this whole runner, so its outcome cannot tell a
# failed `check::default` preflight from a failed `test-all`; run 37685900447
# read `VERDICT: cells ran and FAILED` with 0 cells run. When
# NROS_LANE_STEP_RECORD names a file, every marker line is appended to it too,
# for `lane-stage.py --report` to read (scripts/lib/lane_step_markers.py — one
# parser, and its selftest runs THIS script to hold the format).
mark() {
    printf '%s\n' "$1"
    if [ -n "${NROS_LANE_STEP_RECORD:-}" ]; then
        printf '%s\n' "$1" >> "$NROS_LANE_STEP_RECORD" || true
    fi
}

# Issue 1350 — a stopped lane stops its steps, and says it has no verdict.
#
# Each step runs under `nros_guard_lane`: its own process group, a trap on
# INT/TERM/HUP, and a sentinel that kills the group if this runner — or the
# recipe shell above it, which is the only process `just` signals — goes away.
# Before this, stopping `just ci gate` killed the recipe shell and nothing
# else: this runner was reparented and its `check::build` went on running its
# `-P 32` fan-out, unowned, while the stop read as clean.
# shellcheck source=scripts/build/subtree-guard.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")/../build" && pwd)/subtree-guard.sh"

# The adjacent trap (same issue): `just` exits 1 for a recipe it does not have,
# which reads exactly like a red gate. A lane naming a step that does not exist
# never ran, so it is NO VERDICT (exit 2), decided before any step starts.
for s in "${steps[@]}"; do
    if ! just --show "$s" >/dev/null 2>&1; then
        printf 'ci %s: NO VERDICT — step `%s` is not a recipe, so the lane did not run (issue 1350).\n' \
            "$lane" "$s" >&2
        exit 2
    fi
done

declare -a verdict=() took=()
failed_at=-1
current=-1
lane_start=$(date +%s)
_lane_stopped() {
    [ -n "${NROS_GUARD_CAUGHT:-}" ] || return 0
    [ "$current" -ge 0 ] || return 0
    printf '\nci %s STOPPED by SIG%s during step %d of %d (%s) — NO VERDICT.\n' \
        "$lane" "$NROS_GUARD_CAUGHT" $((current + 1)) "$n" "${steps[$current]}" >&2
    printf '  Its process group was killed; %d step(s) after it did not run.\n' \
        $((n - current - 1)) >&2
}
trap _lane_stopped EXIT
for i in "${!steps[@]}"; do
    if [ "$failed_at" -ge 0 ]; then
        verdict[$i]="NOT RUN"
        took[$i]="-"
        continue
    fi
    mark "$(printf '==> ci %s [%d/%d] %s — started %s' "$lane" $((i + 1)) "$n" "${steps[$i]}" "$(now_utc)")"
    t0=$(date +%s)
    current=$i
    if nros_guard_lane "ci-$lane" just "${steps[$i]}"; then verdict[$i]="ok"; else verdict[$i]="FAILED"; failed_at=$i; fi
    dt=$(( $(date +%s) - t0 ))
    took[$i]="$(fmt_dur "$dt")"
    mark "$(printf '<== ci %s [%d/%d] %s — %s after %s (at %s)' \
        "$lane" $((i + 1)) "$n" "${steps[$i]}" "${verdict[$i]}" "${took[$i]}" "$(now_utc)")"
done
total=$(( $(date +%s) - lane_start ))

printf '\n==> ci %s: step timings\n' "$lane"
for i in "${!steps[@]}"; do
    printf '  %-8s %10s  %s\n' "${verdict[$i]}" "${took[$i]}" "${steps[$i]}"
done
printf '  %-8s %10s\n' "total" "$(fmt_dur "$total")"

if [ "$failed_at" -lt 0 ]; then
    exit 0
fi
skipped=$(( n - failed_at - 1 ))
printf '\nci %s FAILED at step %d of %d (%s).\n' "$lane" $((failed_at + 1)) "$n" "${steps[$failed_at]}"
if [ "$skipped" -gt 0 ]; then
    printf '  %d step(s) did NOT run. This lane stops at the first failure, so a red\n' "$skipped"
    printf '  step WITHDRAWS every step after it, and nothing below it was measured.\n'
    printf '  A red count is not a coverage report (issue 0952).\n'
fi
exit 1
