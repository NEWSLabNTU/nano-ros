#!/usr/bin/env bash
# phase-488 — boot every ported stock rclcpp example and judge how it ended.
#
#   scripts/check-rclcpp-examples-boot.sh <bin-dir> <example>...
#
# Each `<bin-dir>/<example>` is started at once (3 s in total, not 3 s each)
# under the shared escalating deadline (`scripts/lib/deadline.sh`): an example
# that is still spinning when the deadline fires (124, or 137 if it ignored
# SIGTERM) or that finished on its own (0) BOOTED — init, session, every entity
# created. Anything else failed, typically 134: a create that failed aborts
# (`detail::require_created`), which is the regression this run exists for.
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
        0 | 124 | 137) return 0 ;;
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
            "${NROS_DEADLINE[@]}" "$DEADLINE_S" "$dir/$ex" 2>&1 | tail -c 65536 >"$dir/$ex.log"
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
    chmod +x "$t/aborts" "$t/spins" "$t/finishes"
    local ok=0
    run_all "$t" spins finishes 2>/dev/null || {
        echo "selftest: a spinning and a finishing example were reported as failures" >&2
        ok=1
    }
    if run_all "$t" aborts 2>/dev/null; then
        echo "selftest: an aborting example was NOT reported" >&2
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
