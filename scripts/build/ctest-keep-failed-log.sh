#!/usr/bin/env bash
# Run ctest over one build dir and, when it fails, keep that run's log.
#
# Usage: scripts/build/ctest-keep-failed-log.sh <build-dir> [ctest args...]
#
# ctest writes every run's full output to <build-dir>/Testing/Temporary/
# LastTest.log and nothing else, so the NEXT run in that dir overwrites it.
# The usual next run is a rerun of the one test that failed, and that is
# exactly what loses a load-sensitive failure: a backend ctest red at -j8
# under load passed three times alone, and those three reruns overwrote the
# only log holding its message. The console copy had been filtered down
# to the summary line. So a failed run's log is copied to
# <build-dir>/Testing/failed-runs/LastTest-<UTC stamp>.log, which no later
# run writes, and the path is printed.
#
# `--output-on-failure` is always passed, so a failure's output also lands
# in whatever log the caller keeps.
#
# Sweep: git grep -nE '(^|[^a-z_-])ctest( |$)' -- just justfile scripts .github
set -u

if [ "$#" -lt 1 ]; then
    echo "usage: $0 <build-dir> [ctest args...]" >&2
    exit 2
fi
bd="$1"
shift

rc=0
ctest --test-dir "$bd" --output-on-failure "$@" || rc=$?
if [ "$rc" -ne 0 ]; then
    log="$bd/Testing/Temporary/LastTest.log"
    if [ -f "$log" ]; then
        keep_dir="$bd/Testing/failed-runs"
        mkdir -p "$keep_dir"
        kept="$keep_dir/LastTest-$(date -u +%Y%m%dT%H%M%SZ)-$$.log"
        cp "$log" "$kept"
        echo "ctest failed (exit $rc); this run's full log is kept at $kept" >&2
    else
        echo "ctest failed (exit $rc) and wrote no $log to keep" >&2
    fi
fi
exit "$rc"
