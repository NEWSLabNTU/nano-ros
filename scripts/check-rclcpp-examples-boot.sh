#!/usr/bin/env bash
# phase-488 — boot every ported stock rclcpp example and judge how it ended.
#
#   scripts/check-rclcpp-examples-boot.sh <bin-dir> <example>...
#
# Each `<bin-dir>/<example>` is started at once (3 s in total, not 3 s each)
# under the shared escalating deadline (`scripts/lib/deadline.sh`): an example
# that is still spinning when the deadline fires (143, the deadline's SIGTERM,
# or 137 if it ignored that and took the SIGKILL) or that finished on its own
# (0) BOOTED — init, session, every entity created. Anything else failed,
# typically 134: a create that failed aborts (`detail::require_created`), which
# is the regression this run exists for.
#
# The deadline runs with `--preserve-status`, so the status judged is the
# EXAMPLE's, never `timeout`'s own 124. Without it, an abort whose core dump is
# still being written when the deadline fires reads 124, i.e. "still running":
# with a pipe `core_pattern` (systemd-coredump) the dump of a 26 KiB core took
# 6 s on a loaded host (2026-11-26), twice the 3 s deadline, and the selftest's
# own aborting example passed as BOOTED. A real ported example that aborts
# under load would have passed the same way.
# Each log keeps its last 64 KiB in `<bin-dir>/<example>.log` (minimal_client's
# wait loop answers at once on the stub backend and logs per pass).
#
# The classifier's own negative control runs first, on every invocation: a
# fake example that aborts must be reported, one that spins must not.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=lib/deadline.sh
. "$ROOT/scripts/lib/deadline.sh"

DEADLINE_S=3

# booted <exit-code> — 0 when that exit means the example booted.
booted() {
    case "$1" in
        0 | 143 | 137) return 0 ;;
        *) return 1 ;;
    esac
}

# run_all <bin-dir> <example>... — prints one line per failure, returns 1 on any.
run_all() {
    local dir="$1"
    shift
    local pids=() names=() ex i rc fail=0
    for ex in "$@"; do
        test -x "$dir/$ex" || {
            echo "ERROR: $dir/$ex is missing or not executable" >&2
            fail=1
            continue
        }
        (
            "${NROS_DEADLINE[@]}" --preserve-status "$DEADLINE_S" "$dir/$ex" 2>&1 | tail -c 65536 >"$dir/$ex.log"
            exit "${PIPESTATUS[0]}"
        ) &
        pids+=("$!")
        names+=("$ex")
    done
    for i in "${!pids[@]}"; do
        rc=0
        wait "${pids[$i]}" || rc=$?
        if ! booted "$rc"; then
            echo "ERROR: ported ${names[$i]} exited $rc (log: $dir/${names[$i]}.log)" >&2
            tail -5 "$dir/${names[$i]}.log" >&2
            fail=1
        fi
    done
    return "$fail"
}

selftest() {
    local t
    t="$(mktemp -d)" || return 1
    printf '#!/bin/sh\necho aborting\nkill -ABRT $$\n' >"$t/aborts"
    printf '#!/bin/sh\nexec sleep 30\n' >"$t/spins"
    printf '#!/bin/sh\nexit 0\n' >"$t/finishes"
    # Ignores the deadline's SIGTERM, so only the SIGKILL ends it: still a boot.
    printf '#!/bin/sh\ntrap "" TERM\nwhile :; do sleep 0.1; done\n' >"$t/ignores_term"
    # Dies by SIGABRT only AFTER the deadline fired: the shape of an abort whose
    # core dump outlasts the deadline, made deterministic. Plain `timeout`
    # reports this as its own 124.
    printf '#!/bin/sh\ntrap "kill -ABRT \\$\\$" TERM\nwhile :; do sleep 0.1; done\n' >"$t/aborts_late"
    chmod +x "$t/aborts" "$t/spins" "$t/finishes" "$t/ignores_term" "$t/aborts_late"
    local ok=0
    run_all "$t" spins finishes ignores_term 2>/dev/null || {
        echo "selftest: a spinning, a finishing or a TERM-ignoring example was reported as a failure" >&2
        ok=1
    }
    if run_all "$t" aborts 2>/dev/null; then
        echo "selftest: an aborting example was NOT reported" >&2
        ok=1
    fi
    if run_all "$t" aborts_late 2>/dev/null; then
        echo "selftest: an example that aborted AFTER the deadline fired was NOT reported" >&2
        ok=1
    fi
    rm -rf "$t"
    return "$ok"
}

selftest || {
    echo "check-rclcpp-examples-boot: selftest FAILED -- the classifier cannot be trusted" >&2
    exit 1
}
[ "$#" -ge 2 ] || {
    echo "usage: $0 <bin-dir> <example>..." >&2
    exit 2
}
run_all "$@"
