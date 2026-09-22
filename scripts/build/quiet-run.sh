#!/usr/bin/env bash

# One spelling for "run this quietly, but SAY WHY when it fails".
#
# A build recipe that pipes a `cmake --build` to `/dev/null` keeps its log
# readable and, on the day it fails, reports a TALLY with no diagnosis:
# ninja writes `FAILED:` and every compiler diagnostic to STDOUT, so the
# redirect discards exactly the lines a reader needs. The 2026-09-22 nightly
# (run 35698520560, job 106651319044) printed
# `ThreadX-RV64 rust leaves: 0/12 ok, 12 failed` over a 43,652-line log holding
# ZERO occurrences of `FAILED:`, `CMake Error` or `ninja: build stopped` —
# twelve failures whose cause was unknowable from the artifact. A lane with a
# verdict and no diagnosis cannot be triaged, which is the same defect class
# CLAUDE.md records for a uniformly-red lane.
#
# Usage:  nros_run_quiet "<label>" <command> [args...]
#
# Success is silent, as the redirect was. Failure prints the tail of the
# captured output to stderr under a header naming the label and the exit
# status. The command may be a shell function, so this is a call rather than
# `env`; a `VAR=value` prefix on the `nros_run_quiet` call reaches it exactly
# as it reached the direct call.
NROS_QUIET_RUN_TAIL="${NROS_QUIET_RUN_TAIL:-120}"

nros_run_quiet() {
    local label="$1"
    shift
    local log rc=0
    # issue 1249 — a status this function MEANS to inspect must not die at the
    # assignment under `set -e`.
    log="$(mktemp)" || return 1
    "$@" > "$log" 2>&1 || rc=$?
    if [ "$rc" -ne 0 ]; then
        local lines
        lines="$(wc -l < "$log")"
        {
            echo "  FAILED: $label (exit $rc)"
            if [ "$lines" -gt "$NROS_QUIET_RUN_TAIL" ]; then
                echo "  last $NROS_QUIET_RUN_TAIL of $lines line(s) of captured output:"
            else
                echo "  captured output ($lines line(s)):"
            fi
            tail -n "$NROS_QUIET_RUN_TAIL" "$log"
        } >&2
    fi
    rm -f "$log"
    return "$rc"
}
