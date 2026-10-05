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

declare -a verdict=() took=()
failed_at=-1
lane_start=$(date +%s)
for i in "${!steps[@]}"; do
    if [ "$failed_at" -ge 0 ]; then
        verdict[$i]="NOT RUN"
        took[$i]="-"
        continue
    fi
    printf '==> ci %s [%d/%d] %s — started %s\n' "$lane" $((i + 1)) "$n" "${steps[$i]}" "$(now_utc)"
    t0=$(date +%s)
    if just "${steps[$i]}"; then verdict[$i]="ok"; else verdict[$i]="FAILED"; failed_at=$i; fi
    dt=$(( $(date +%s) - t0 ))
    took[$i]="$(fmt_dur "$dt")"
    printf '<== ci %s [%d/%d] %s — %s after %s (at %s)\n' \
        "$lane" $((i + 1)) "$n" "${steps[$i]}" "${verdict[$i]}" "${took[$i]}" "$(now_utc)"
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
