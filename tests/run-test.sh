#!/usr/bin/env bash
# Wrapper for non-cargo tests (QEMU semihosting, shell scripts).
# Captures output to a log file and prints a one-line summary.
#
# Usage:
#   ./tests/run-test.sh --name <name> --log <path> [--verbose] [--qemu] -- <command...>
#
# Options:
#   --name     Test name for summary line
#   --log      Path to log file
#   --verbose  Stream output live (tee) instead of capturing silently
#   --qemu     Parse QEMU semihosting [PASS]/[FAIL] markers for counts
#   --timeout  Deadline in seconds (default 120 with --qemu, none otherwise)
#
# Issue 1865 — a QEMU run gets a DEADLINE, always. A semihosting image that
# never reaches its exit call never ends, and QEMU killed by a signal exits 0:
# `test-rtic-main-e2e` sat 25 minutes on an image whose exit callback was never
# dispatched, and when the QEMU was killed by hand this wrapper printed
# `[PASS] … 0 passed, 0 failed`. Under the deadline the run ends, and the
# deadline's own status (124/137) is a FAIL that says what happened.
set -euo pipefail

name=""
logfile=""
verbose=false
qemu=false
timeout_s=""
cmd=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --name)   name="$2"; shift 2 ;;
        --log)    logfile="$2"; shift 2 ;;
        --verbose) verbose=true; shift ;;
        --qemu)   qemu=true; shift ;;
        --timeout) timeout_s="$2"; shift 2 ;;
        --)       shift; cmd=("$@"); break ;;
        *)        echo "Unknown option: $1" >&2; exit 1 ;;
    esac
done

if [[ -z "$name" || -z "$logfile" || ${#cmd[@]} -eq 0 ]]; then
    echo "Usage: $0 --name <name> --log <path> [--verbose] [--qemu] -- <command...>" >&2
    exit 1
fi

mkdir -p "$(dirname "$logfile")"

if [[ -z "$timeout_s" ]] && $qemu; then
    timeout_s=120
fi
if [[ -n "$timeout_s" ]]; then
    # shellcheck source=../scripts/lib/deadline.sh
    . "$(dirname "$0")/../scripts/lib/deadline.sh"
    cmd=("${NROS_DEADLINE[@]}" "$timeout_s" "${cmd[@]}")
fi

start=$(date +%s)
rc=0

if $verbose; then
    "${cmd[@]}" 2>&1 | tee "$logfile" || rc=$?
else
    "${cmd[@]}" > "$logfile" 2>&1 || rc=$?
fi

elapsed=$(( $(date +%s) - start ))

if [[ -n "$timeout_s" && ( $rc -eq 124 || $rc -eq 137 ) ]]; then
    echo "  [FAIL] $name  did not exit within ${timeout_s}s (deadline, rc=$rc)"
    if ! $verbose; then
        echo "  --- Output (last 20 lines) ---"
        tail -20 "$logfile" | sed 's/^/  /'
        echo "  --- Full log: $logfile ---"
    fi
    exit 1
fi

if $qemu; then
    passed=$(grep -c '\[PASS\]' "$logfile" 2>/dev/null || true)
    failed=$(grep -c '\[FAIL\]' "$logfile" 2>/dev/null || true)
    : "${passed:=0}" "${failed:=0}"
    if [[ $rc -eq 0 && $failed -eq 0 ]]; then
        echo "  [PASS] $name  ${passed} passed, ${failed} failed  (${elapsed}s)"
    else
        echo "  [FAIL] $name  ${passed} passed, ${failed} failed  (${elapsed}s)"
        if ! $verbose; then
            echo "  --- Output (last 20 lines) ---"
            tail -20 "$logfile" | sed 's/^/  /'
            echo "  --- Full log: $logfile ---"
        fi
        exit 1
    fi
else
    if [[ $rc -eq 0 ]]; then
        echo "  [PASS] $name  (${elapsed}s)"
    else
        echo "  [FAIL] $name  (${elapsed}s)"
        if ! $verbose; then
            echo "  --- Output (last 20 lines) ---"
            tail -20 "$logfile" | sed 's/^/  /'
            echo "  --- Full log: $logfile ---"
        fi
        exit 1
    fi
fi
