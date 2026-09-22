#!/usr/bin/env bash
#
# `nros_run_quiet` must be SILENT on success and must PRINT on failure.
#
# The rule exists because the opposite — `cmake --build … > /dev/null` — is how
# the ThreadX-RV64 nightly came to report `0/12 ok, 12 failed` over a
# 43,652-line log containing no `FAILED:` line at all (run 35698520560, job
# 106651319044): ninja writes its failures and every compiler diagnostic to
# STDOUT, so the redirect discarded exactly what a reader needed. A test that
# only checked the exit status would pass against that redirect, so each case
# below asserts what reached the STREAMS.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# shellcheck source=scripts/build/quiet-run.sh
source "$root/scripts/build/quiet-run.sh"

fail() {
    echo "[FAIL] $*" >&2
    exit 1
}

noisy() {
    echo "a stdout line"
    echo "a stderr line" >&2
    return "$1"
}

# 1. Success prints NOTHING on either stream — the quiet the redirect bought.
out="$(nros_run_quiet "success case" noisy 0 2>&1)"
[ -z "$out" ] || fail "a successful run must be silent, got: $out"

# 2. Failure prints the captured STDOUT, which is the stream ninja fails on.
rc=0
out="$(nros_run_quiet "failure case" noisy 7 2>&1)" || rc=$?
[ "$rc" -eq 7 ] || fail "the command's exit status must be forwarded, got $rc"
case "$out" in
    *"a stdout line"*) ;;
    *) fail "a failing run must print its captured stdout, got: $out" ;;
esac
case "$out" in
    *"a stderr line"*) ;;
    *) fail "a failing run must print its captured stderr, got: $out" ;;
esac
case "$out" in
    *"failure case"*) ;;
    *) fail "the failure header must name the label, got: $out" ;;
esac

# 3. The report goes to STDERR, so a caller capturing stdout still sees it.
rc=0
out="$(nros_run_quiet "stderr case" noisy 1 2>/dev/null)" || rc=$?
[ "$rc" -eq 1 ] || fail "expected exit 1, got $rc"
[ -z "$out" ] || fail "the failure report belongs on stderr, not stdout: $out"

# 4. A long output is tailed rather than dropped: the LAST lines survive,
#    because that is where `FAILED:` and the diagnostic sit.
long() {
    seq 1 500
    return 1
}
rc=0
out="$(NROS_QUIET_RUN_TAIL=10 nros_run_quiet "tail case" long 2>&1)" || rc=$?
[ "$rc" -eq 1 ] || fail "expected exit 1, got $rc"
case "$out" in
    *"500"*) ;;
    *) fail "the tail must keep the LAST lines, got: $out" ;;
esac
case "$out" in
    *"last 10 of 500 line(s)"*) ;;
    *) fail "a truncated report must say it truncated, got: $out" ;;
esac

echo "[PASS] quiet-run: 4 case(s) held"
