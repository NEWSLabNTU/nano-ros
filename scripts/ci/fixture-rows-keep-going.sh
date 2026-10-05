#!/usr/bin/env bash
# Issue 1671 — run a lane step with fixture rows built INDEPENDENTLY, then own
# the verdict on the rows that failed.
#
#   fixture-rows-keep-going.sh <ledger> -- <command…>
#
# Sets `NROS_FIXTURE_FAILED_ROWS=<ledger>` for the command, so every row the
# shared builders run (`nros_fixture_row`, scripts/build/fixture-row-ledger.sh)
# goes on past a failed row and records it instead of ending the lane. Then:
#
#   * the command itself failed  -> its status, after listing any failed rows
#     (a failure that is not a row — a missing SDK, a sync error — still stops
#     the lane where it happens, exactly as before);
#   * it succeeded but rows failed -> exit 1, naming every one. The lane goes
#     RED; what changed is that every OTHER row was built, so the cells whose
#     fixtures exist can still report.
#   * neither                     -> 0.
#
# The ledger is truncated first, so a row that failed on a previous run of the
# same step cannot be reported again (or hide behind a stale file).
set -uo pipefail

if [ "$#" -lt 3 ] || [ "$2" != "--" ]; then
    echo "usage: $0 <ledger> -- <command…>" >&2
    exit 2
fi
ledger="$1"
shift 2

mkdir -p "$(dirname "$ledger")"
: > "$ledger"

rc=0
NROS_FIXTURE_FAILED_ROWS="$ledger" "$@" || rc=$?

failed=0
[ -s "$ledger" ] && failed="$(grep -c . "$ledger")"
if [ "$failed" -gt 0 ]; then
    echo ""
    echo "fixture-rows-keep-going: ${failed} fixture row(s) FAILED to build; every other row was built (issue 1671):"
    sed 's/^/  - /' "$ledger"
    if [ -n "${GITHUB_STEP_SUMMARY:-}" ]; then
        {
            echo "### ${failed} fixture row(s) failed to build"
            echo ""
            echo "The other rows were built, so their cells still report (issue 1671)."
            echo ""
            sed 's/^/- `/; s/\t/` /' "$ledger"
        } >> "$GITHUB_STEP_SUMMARY"
    fi
fi

if [ "$rc" -ne 0 ]; then
    exit "$rc"
fi
if [ "$failed" -gt 0 ]; then
    exit 1
fi
exit 0
