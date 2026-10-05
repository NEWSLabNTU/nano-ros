#!/usr/bin/env bash
#
# Summarise the per-row wall clock a fixture build log carries (issue 1500).
#
# `fixtures-build.sh` and `workspace-fixtures-build.sh` print one
# `row-time: <row> <N>s` line per row they finish. The logs themselves are tens
# of thousands of lines and go to an artifact; this prints the slowest rows and
# the total into the job log, where a reader looking at a slow step is already.
#
# A measurement, never a verdict: it exits 0 whatever it finds, including a log
# with no rows (a build that died before its first row finished).
#
# Usage:  scripts/ci/row-times.sh <log> [top-N]
set -uo pipefail

log="${1:?usage: row-times.sh <log> [top-N]}"
top="${2:-25}"

[ -f "$log" ] || { echo "row-times: no log at $log"; exit 0; }

echo "::group::slowest fixture rows — $(basename "$log")"
awk '/row-time: / { t = $NF; sub(/s$/, "", t); print t, $(NF - 1) }' "$log" \
    | sort -rn \
    | awk -v top="$top" '
        { n++; sum += $1; if (n <= top) printf "  %6ds  %s\n", $1, $2 }
        END {
            if (n == 0) { print "  (no row finished)"; exit }
            printf "  %d row(s), %ds summed (rows run serially within a workspace group)\n", n, sum
        }'
echo "::endgroup::"
exit 0
