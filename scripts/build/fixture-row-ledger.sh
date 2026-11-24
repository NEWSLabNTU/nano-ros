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

# The same contract for a unit that is not a shell function call: a make
# recipe line or a stage the caller already ran. Records <label> with <rc> and
# returns 0 when the lane keeps going; otherwise returns <rc>, so the caller's
# `exit` behaves as before. (`nros_fixture_row` above runs the command for
# you; this is for callers that already hold the status.)
nros_fixture_row_failed() {
    local label="$1" rc="$2"
    if [ -z "${NROS_FIXTURE_FAILED_ROWS:-}" ]; then
        return "$rc"
    fi
    printf '%s\trc=%s\n' "$label" "$rc" >> "$NROS_FIXTURE_FAILED_ROWS"
    printf 'FIXTURE ROW FAILED (rc=%s), kept going (issue 1671): %s\n' "$rc" "$label" >&2
    return 0
}

# The RUN side of the contract. A lane that kept going built every row it
# could, and recorded the rest in its ledger; the step that then runs the
# cells names that ledger as `NROS_FIXTURE_BUILD_LEDGER`. A preflight that
# finds fixtures missing or stale asks this before it stops the run:
#
#   ledger unset or empty -> 1: nothing failed to build, so a missing fixture
#                            is a real precondition failure, exactly as before;
#   ledger names rows     -> 0, after printing them: the build already went red
#                            on them, and every OTHER cell still deserves its
#                            verdict. Each missing fixture fails its own cell
#                            (`unmet!`/`require_*`, never a skip), so nothing
#                            turns green that was not.
nros_fixture_build_ledger_lets_cells_run() {
    local what="$1" ledger="${NROS_FIXTURE_BUILD_LEDGER:-}"
    if [ -z "$ledger" ] || [ ! -s "$ledger" ]; then
        return 1
    fi
    echo "" >&2
    echo "$what: the lane's build kept going past $(grep -c . "$ledger") failed row(s) (issue 1671):" >&2
    sed 's/^/  - /' "$ledger" >&2
    echo "  The cells run anyway: every fixture reported above fails its own cell," >&2
    echo "  and the build step is already red on these rows." >&2
    return 0
}
