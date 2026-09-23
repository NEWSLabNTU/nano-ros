#!/usr/bin/env bash
#
# Issue 1353, step 1 — say how much disk was used and BY WHAT.
#
# The lanes that die of `No space left on device` leave an artifact that cannot
# answer the next question. Sometimes cargo names the write that failed; often
# the log is truncated mid-compile, because when the disk is gone the log has
# nowhere to land either. In both cases nothing records what the space went on,
# which is exactly what the issue asks for before anyone chooses a fix:
#
#   "Add a disk report … (`df -h` before `just check build` and after it, plus
#    `du -sh` of the cargo target dir, the sccache dir and `third-party/`) so
#    the next failure says how much was used and by what."
#
# ...and the 2026-09-17/18 section extends that to `host-tests`, which fills the
# same disk without entering the compile tier at all.
#
# This is a MEASUREMENT, not a remedy. It never fails a job: every probe is
# allowed to fail, because a report that can break the lane it reports on would
# be worse than no report. It is deliberately quiet enough to run twice per
# step (before, and `always()` after) without burying the log.
#
# Usage:  scripts/ci/disk-report.sh "<label>"
set -uo pipefail

label="${1:-disk}"

echo "::group::disk report — ${label}"

echo "--- df -h (workspace + root + tmp) ---"
# Three paths, but often one filesystem — dedupe so the report does not
# repeat itself, keeping df's header.
df -h "${GITHUB_WORKSPACE:-$PWD}" / /tmp 2>/dev/null | awk 'NR==1 || !seen[$0]++' || df -h || true

echo "--- du -sh of the usual suspects ---"
# One line per path that exists. `du` on a missing path is noise, and a
# `du` that fails (permissions, a vanishing build dir) must not end the run —
# issue 1249: the status is inspected, so it never rides on an assignment.
for p in \
    "${GITHUB_WORKSPACE:-$PWD}/target" \
    "${GITHUB_WORKSPACE:-$PWD}/packages/cli/target" \
    "${GITHUB_WORKSPACE:-$PWD}/build" \
    "${GITHUB_WORKSPACE:-$PWD}/third-party" \
    "${GITHUB_WORKSPACE:-$PWD}/examples" \
    "${SCCACHE_DIR:-/tmp/sccache}" \
    "${HOME}/.nros" \
    "${HOME}/.cargo" \
    "${CARGO_HOME:-}" \
; do
    [ -n "$p" ] || continue
    [ -d "$p" ] || continue
    rc=0
    out="$(du -sh "$p" 2>/dev/null)" || rc=$?
    if [ "$rc" -eq 0 ] && [ -n "$out" ]; then
        printf '%s\n' "$out"
    else
        printf '%s\t(du failed, rc=%s)\n' "$p" "$rc"
    fi
done

# The biggest children of the workspace target dir, when there is one: this is
# where the 2026-09-12 and 2026-09-17 measurements both landed
# (`target/debug/deps/…rmeta`), so a size breakdown there is the first thing a
# reader wants after the totals.
tgt="${GITHUB_WORKSPACE:-$PWD}/target"
if [ -d "$tgt" ]; then
    echo "--- biggest children of target/ ---"
    du -sh "$tgt"/* 2>/dev/null | sort -rh | head -10 || true
fi

# ...and of `examples/`, because the first real measurement made that the
# LARGER half: run 35726999134 reported 42 G there BEFORE `just ci tier1`
# started, against 22 G of free disk for the tier to work in. A single total
# cannot say which fixture families own it, which is the question step 3 of
# issue 1353 has to answer before pruning anything.
ex="${GITHUB_WORKSPACE:-$PWD}/examples"
if [ -d "$ex" ]; then
    echo "--- biggest children of examples/ ---"
    du -sh "$ex"/* 2>/dev/null | sort -rh | head -12 || true
fi

echo "::endgroup::"

# ---------------------------------------------------------------------------
# A CHANNEL THAT SURVIVES THE RUNNER'S DEATH — issue 1353, phase-466 W4.
#
# Everything above goes into the job LOG, and on the scheduled `gate` lane the
# job log is exactly what this failure destroys. Measured on run 35808783184
# (2026-09-23): step 22 `Disk report (before check build)` reported SUCCESS,
# step 23 `just check build` killed the runner with
# `IOException: No space left on device` on its own `_diag/Worker_*.log`, and
# the job's log blob came back `BlobNotFound`. Steps 24+ — including the
# `always()` AFTER report — never ran, because `always()` only binds while a
# runner is alive to honour it. So the measurement that exists to explain this
# failure is unreadable on the one lane that most needs it.
#
# TWO channels, because neither is certain on its own and the issue says so:
#
#   * `$GITHUB_STEP_SUMMARY` — the runner uploads a step's summary file when
#     that STEP completes, not when the job does. The BEFORE report completes
#     successfully, so its numbers are already server-side before the compile
#     tier starts. This is the one expected to work.
#   * `::notice::` — the annotation channel. This issue is readable at all
#     because an annotation carried the `IOException` out of a job whose log
#     was lost, so the channel demonstrably survives; whether a workflow-emitted
#     notice rides it the same way is NOT established, and the issue names
#     finding out as the cheap experiment. Emitting one costs a line.
#
# Both carry a one-line SUMMARY, not the full report: what is wanted after a
# runner death is "how much was free, and what held the disk", and a summary
# that stays short is one a reader gets to before the noise.
_summary_line() {
    local used_pct avail biggest
    used_pct="$(df -h "${GITHUB_WORKSPACE:-$PWD}" 2>/dev/null | awk 'NR==2 {print $5}')"
    avail="$(df -h "${GITHUB_WORKSPACE:-$PWD}" 2>/dev/null | awk 'NR==2 {print $4}')"
    # The three biggest of the paths measured above, as `size path` pairs, so
    # the line says what held the disk and not only how much was left.
    biggest="$(du -sh \
        "${GITHUB_WORKSPACE:-$PWD}/target" \
        "${GITHUB_WORKSPACE:-$PWD}/packages/cli/target" \
        "${GITHUB_WORKSPACE:-$PWD}/build" \
        "${GITHUB_WORKSPACE:-$PWD}/examples" \
        2>/dev/null | sort -rh | head -3 \
        | awk '{n = split($2, p, "/"); printf "%s%s %s", (NR > 1 ? "; " : ""), $1, p[n]}')"
    printf '%s used, %s free — %s' "${used_pct:-?}" "${avail:-?}" "${biggest:-(no sizes)}"
}

_summary="$(_summary_line 2>/dev/null)" || _summary=""
if [ -n "$_summary" ]; then
    # `::notice::` — the experiment. If the next scheduled `gate` failure shows
    # this under `check-runs/<jid>/annotations` while the log is still
    # `BlobNotFound`, the annotation channel is confirmed; if it does not, the
    # step summary below is what carried the numbers and this line can go.
    printf '::notice title=disk %s::%s\n' "$label" "$_summary"
    # `$GITHUB_STEP_SUMMARY` — the one expected to survive. Appended, never
    # truncated: both the BEFORE and the AFTER report write here, and a reader
    # comparing them is the whole point.
    if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
        printf -- '- **disk %s** — %s\n' "$label" "$_summary" \
            >>"$GITHUB_STEP_SUMMARY" 2>/dev/null || true
    fi
fi
