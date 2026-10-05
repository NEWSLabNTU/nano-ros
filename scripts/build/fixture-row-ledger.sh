# shellcheck shell=bash
# Issue 1671 — build ONE fixture row, and when the lane says so, keep going
# past a row that fails instead of ending the lane on it.
#
# A nightly platform job builds every row, then runs the cells. The builders
# were `set -e` row by row, so the FIRST row that failed to build ended the job
# before any cell ran — including every cell whose fixture built fine. One
# row's link failure (`realtime-cpp`'s `.bss` overflow, run 36977810939) became
# no verdict about every other FreeRTOS cell, and issue 1657 sat on main behind
# it for as long as that row stayed red.
#
# THE CONTRACT. `NROS_FIXTURE_FAILED_ROWS` names a ledger file.
#
#   unset  -> `nros_fixture_row` is exactly the command it wraps: same exit
#             status, same `set -e` behaviour. Every developer invocation and
#             every lane that does not ask is byte-for-byte what it was.
#   set    -> a failing row is APPENDED to the ledger (one line: the row, its
#             exit status) and the call returns 0, so the builder goes on to
#             the next row. Whoever set the variable owns the verdict: it must
#             read the ledger at the end and fail when it is not empty
#             (`scripts/ci/fixture-rows-keep-going.sh` is that owner). The
#             ledger is the record, so a kept-going failure is never a pass.
#
# WHY A BACKGROUND JOB AND NOT `cmd || rc=$?`. A function called on the left of
# `||` runs with `set -e` SUSPENDED for its whole body (bash: "-e is ignored
# within a compound command or function executed in a context where -e is
# being ignored"), and a `( … )` subshell inherits that. So the obvious spelling
# would turn every row builder that relies on `set -e` into one that ignores its
# own failures and returns the status of its LAST command — kept-going rows
# that silently "pass". `( cmd ) & wait $!` runs the row in its own process with
# `-e` intact, and `wait` hands back its real status. (A background job's stdin
# is /dev/null; no row builder reads stdin.)
#
# Exported (`export -f`) by the builders that run rows in `make` workers, whose
# leaves are fresh bashes holding only what was exported.

nros_fixture_row() {
    local label="$1"
    shift
    if [ -z "${NROS_FIXTURE_FAILED_ROWS:-}" ]; then
        "$@"
        return
    fi
    local rc=0
    ( "$@" ) &
    wait "$!" || rc=$?
    [ "$rc" -eq 0 ] && return 0
    printf '%s\trc=%s\n' "$label" "$rc" >> "$NROS_FIXTURE_FAILED_ROWS"
    printf 'FIXTURE ROW FAILED (rc=%s), kept going (issue 1671): %s\n' "$rc" "$label" >&2
    return 0
}
