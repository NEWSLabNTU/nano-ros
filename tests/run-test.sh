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
set -euo pipefail

name=""
logfile=""
verbose=false
qemu=false
cmd=()

while [[ $# -gt 0 ]]; do
    case "$1" in
        --name)   name="$2"; shift 2 ;;
        --log)    logfile="$2"; shift 2 ;;
        --verbose) verbose=true; shift ;;
        --qemu)   qemu=true; shift ;;
        --)       shift; cmd=("$@"); break ;;
        *)        echo "Unknown option: $1" >&2; exit 1 ;;
    esac
done

if [[ -z "$name" || -z "$logfile" || ${#cmd[@]} -eq 0 ]]; then
    echo "Usage: $0 --name <name> --log <path> [--verbose] [--qemu] -- <command...>" >&2
    exit 1
fi

mkdir -p "$(dirname "$logfile")"

start=$(date +%s)
rc=0

if $verbose; then
    "${cmd[@]}" 2>&1 | tee "$logfile" || rc=$?
else
    "${cmd[@]}" > "$logfile" 2>&1 || rc=$?
fi

elapsed=$(( $(date +%s) - start ))

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
