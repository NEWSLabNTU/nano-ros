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

# The transcript — a file this report also lands in, so that a workflow step
# can hand it to the artifact service while the runner is still alive. See
# `disk-transcript.sh` for why neither channel phase-466 W4 added survives the
# failure this script exists to describe.
# shellcheck source=scripts/ci/disk-transcript.sh
. "$(dirname "${BASH_SOURCE[0]}")/disk-transcript.sh"
_transcript="$(nros_disk_transcript_open)"

{

echo "::group::disk report — ${label}"
printf '(%s)\n' "$(date -u '+%Y-%m-%dT%H:%M:%SZ' 2>/dev/null || echo 'no date')"

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

# EVERY top-level entry of the checkout, biggest first — because the hand-written
# list above has a REACH narrower than the question it is asked (issue 0196's
# shape, one lane over). Issue 1353's own 2026-09-23 section records the gap and
# misattributes it: "everything the report measures sums to ~76 G [of 146 G] …
# the rest is outside the checkout and outside this report's reach." Some of it
# is not outside the checkout at all. `nros_scoped_target_dir <suffix>` puts a
# gate's cargo scratch at `$PWD/target-<suffix>`, a SIBLING of `target/` that no
# line above names, and the build tier makes several of them —
# `target-embedded` (`check workspace-all`'s concurrent embedded clippy),
# `target-param-services` (`check compile-smoke`, issue 1382),
# `target-excluded-tests`, plus four more the root `.gitignore` enumerates.
# A list of paths cannot answer "what is on this disk"; a sweep can, and it
# costs one more `du` pass on a nightly bracket.
#
# Dotted entries are in the sweep too, and `.git` is the reason: this job
# fetches the history of twenty submodules, and a submodule's object store
# lives under `.git/modules`, which is neither `third-party/` nor anything else
# the list names.
echo "--- every top-level entry of the checkout, biggest first ---"
du -sh "${GITHUB_WORKSPACE:-$PWD}"/* "${GITHUB_WORKSPACE:-$PWD}"/.[!.]* 2>/dev/null \
    | sort -rh | head -15 || true

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

# `tee` rather than a redirect: the report keeps going to the job log, which is
# what a reader uses on every lane whose runner lives. `/dev/null` when the
# transcript could not be opened.
} 2>&1 | tee -a "${_transcript:-/dev/null}"

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
# THE EXPERIMENT HAS RUN, AND BOTH OF W4's CHANNELS LOST — scheduled `gate`
# run 35945635228, job 107462875362 (2026-09-24). That job emitted a notice
# from step 22 (this report), step 23 (the reclaim) and step 25 (this report
# again), all three of which COMPLETED SUCCESSFULLY, and its annotation list
# holds exactly one entry: the runner's `IOException`, which the service wrote
# rather than the runner. Its run page renders no job summary at all. The
# annotation channel is not broken — the sibling `host-tests` job
# 107478998501, whose runner lived, shows all three notices — it is simply
# flushed by the runner, so it dies when the runner does, and so does the step
# summary.
#
# THREE channels now, the third being the one the issue names as the fallback:
#
#   * the TRANSCRIPT file plus a workflow step that uploads it as an artifact.
#     An artifact is a completed server-side transaction, so once the upload
#     step finishes the numbers exist without the runner. This is the one
#     expected to work; `disk-transcript.sh` holds the path and the reasoning.
#   * `$GITHUB_STEP_SUMMARY` — kept. It costs a line, it is the pleasant way to
#     read a lane that DID survive, and it is now known not to be the answer
#     for one that did not.
#   * `::notice::` — kept, for the same reason and with the same caveat. On a
#     lane whose runner lives it is the fastest thing to read
#     (`check-runs/<jid>/annotations`, no log download).
#
# The two runner-side channels carry a one-line SUMMARY, not the full report:
# what is wanted after a
# runner death is "how much was free, and what held the disk", and a summary
# that stays short is one a reader gets to before the noise.
_summary_line() {
    local used_pct avail biggest
    used_pct="$(df -h "${GITHUB_WORKSPACE:-$PWD}" 2>/dev/null | awk 'NR==2 {print $5}')"
    avail="$(df -h "${GITHUB_WORKSPACE:-$PWD}" 2>/dev/null | awk 'NR==2 {print $4}')"
    # The three biggest of the paths measured above, as `size path` pairs, so
    # the line says what held the disk and not only how much was left. The path
    # is rendered RELATIVE to the workspace, not as a basename: `target` and
    # `packages/cli/target` share a basename, and on 2026-09-23 the after-report
    # rendered them both as `target` — one at 13 G and the other at 6.9 G, with
    # nothing in the line to say which one a reader was looking at.
    biggest="$(du -sh \
        "${GITHUB_WORKSPACE:-$PWD}/target" \
        "${GITHUB_WORKSPACE:-$PWD}/packages/cli/target" \
        "${GITHUB_WORKSPACE:-$PWD}/build" \
        "${GITHUB_WORKSPACE:-$PWD}/examples" \
        2>/dev/null | sort -rh | head -3 \
        | awk -v root="${GITHUB_WORKSPACE:-$PWD}/" \
            '{ path = $2; if (index(path, root) == 1) path = substr(path, length(root) + 1);
               printf "%s%s %s", (NR > 1 ? "; " : ""), $1, path }')"
    printf '%s used, %s free — %s' "${used_pct:-?}" "${avail:-?}" "${biggest:-(no sizes)}"
}

_summary="$(_summary_line 2>/dev/null)" || _summary=""
if [ -n "$_summary" ]; then
    # `::notice::` — fast to read on a lane whose runner lived, and measured
    # not to survive one that died (see the block above).
    printf '::notice title=disk %s::%s\n' "$label" "$_summary"
    # `$GITHUB_STEP_SUMMARY` — same standing. Appended, never truncated: both
    # the BEFORE and the AFTER report write here, and a reader comparing them
    # is the whole point.
    if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
        printf -- '- **disk %s** — %s\n' "$label" "$_summary" \
            >>"$GITHUB_STEP_SUMMARY" 2>/dev/null || true
    fi
    # The transcript — the channel that survives, once a step has uploaded it.
    # The summary goes in too, so an artifact read on its own answers the
    # first question without re-reading the `du` block.
    if [ -n "${_transcript:-}" ]; then
        printf -- 'SUMMARY disk %s — %s\n\n' "$label" "$_summary" \
            >>"$_transcript" 2>/dev/null || true
    fi
fi

exit 0
