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

echo "::endgroup::"
