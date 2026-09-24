#!/usr/bin/env bash
#
# Issue 1353 — the ONE spelling of "where the disk numbers are written so that
# they can leave the runner". Sourced by `disk-report.sh` and
# `reclaim-disk.sh`; it is not executable on its own.
#
# WHY A FILE AT ALL, when both scripts already print.
#
# The failure this measures KILLS THE RUNNER, and a dead runner uploads
# nothing it had not already handed to the service. Phase-466 W4 added two
# channels on the reasoning that one of them would survive, and named finding
# out as the cheap experiment. The experiment has now run, on scheduled `gate`
# run 35945635228 / job 107462875362 (2026-09-24), and BOTH answers are no:
#
#   * `::notice::` does NOT survive. The job emitted three of them from steps
#     22, 23 and 25, all of which completed successfully, and
#     `check-runs/107462875362/annotations` holds exactly one annotation — the
#     runner's own `IOException`, which the SERVICE wrote, not the runner. The
#     channel itself works: the sibling `host-tests` job 107478998501, whose
#     runner survived, shows all three notices in its annotations. So a
#     workflow-emitted annotation is buffered on the runner and dies with it.
#   * `$GITHUB_STEP_SUMMARY` does NOT survive either. The run page renders no
#     job summary for that job at all, although the BEFORE report, the reclaim
#     and the AFTER report each completed.
#
# What is left is the channel the issue names as the fallback: "an artifact
# uploaded before the compile tier". An artifact upload is a completed
# server-side transaction — once the upload STEP finishes, the blob exists
# independently of the runner. So both scripts append everything they print to
# one transcript file, and each workflow uploads it in a step of its own
# immediately after. Whatever the runner survives long enough to write is then
# readable afterwards, and the upload that never happens costs only the
# numbers it would have added.
#
# The path has ONE home, here. A workflow that wants to upload the transcript
# names `$NROS_DISK_TRANSCRIPT`, which `nros_disk_transcript_open` publishes to
# `$GITHUB_ENV`, rather than repeating the path — a second spelling of a path
# is how the upload step ends up pointing at a file nobody writes.

# The transcript's path: whatever the caller set, else a file in the runner's
# temp dir. `$RUNNER_TEMP` rather than the workspace, because the workspace is
# the tree the lanes build in and a report has no business being build input.
nros_disk_transcript_path() {
    if [ -n "${NROS_DISK_TRANSCRIPT:-}" ]; then
        printf '%s' "${NROS_DISK_TRANSCRIPT}"
        return 0
    fi
    printf '%s/nros-disk-transcript.md' "${RUNNER_TEMP:-${TMPDIR:-/tmp}}"
}

# Prints the transcript path if it can be written, and NOTHING if it cannot —
# so a caller can write `tee -a "${t:-/dev/null}"` and a read-only or missing
# directory degrades to "no transcript" rather than to a failed report. Same
# rule the two callers already state for every probe they run: a measurement
# that can break the lane it measures is worse than no measurement.
nros_disk_transcript_open() {
    local p dir
    p="$(nros_disk_transcript_path)"
    dir="$(dirname "$p")"
    mkdir -p "$dir" 2>/dev/null || true
    : >>"$p" 2>/dev/null || return 0
    # Publish it to the steps that follow, so the upload step in each workflow
    # names the variable instead of re-deriving the path.
    if [ -n "${GITHUB_ENV:-}" ]; then
        printf 'NROS_DISK_TRANSCRIPT=%s\n' "$p" >>"${GITHUB_ENV}" 2>/dev/null || true
    fi
    printf '%s' "$p"
}
