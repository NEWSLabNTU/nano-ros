#!/usr/bin/env bash
# Retry wrapper for non-deterministic rustc crashes / ICEs (issue 0115).
#
# Some hosts intermittently crash rustc (SIGSEGV / SIGABRT / "internal compiler
# error" / "unexpectedly panicked") under the heavy, mixed parallel load of the
# fixture build — cargo + cmake + ninja + nested probe/corrosion cargos all
# compiling at once. The same crate compiles fine on a retry (the failures are
# non-deterministic and hop between unrelated crates: paste, toml, nros-macros,
# nros, nros-cpp…), so a bounded retry transparently recovers.
#
# Used as a cargo RUSTC_WRAPPER, so cargo invokes us as:
#   rustc-retry.sh <rustc> <args...>
#
# Design points:
#   * Each attempt's stdout/stderr is buffered to a temp file and only the
#     FINAL (successful or last) attempt's output is emitted, so a retried
#     crash never double-feeds cargo's stdout artifact-JSON stream.
#   * ONLY crashes/ICEs retry. A normal compile error (exit 101 with no ICE
#     signature) is forwarded immediately — never retried — so real build
#     failures still fail fast.
#   * Retries are bounded (NROS_RUSTC_RETRY, default 3). On a genuinely
#     deterministic crash it gives up after the cap and forwards the failure.
#   * Disable entirely with NROS_RUSTC_RETRY=1 (a single attempt = passthrough).
set -uo pipefail

max="${NROS_RUSTC_RETRY:-3}"
case "$max" in
    '' | *[!0-9]*) max=3 ;;
esac
[ "$max" -ge 1 ] || max=1

# Best-effort crate name for diagnostics (the value after --crate-name).
crate="?"
prev=""
for a in "$@"; do
    if [ "$prev" = "--crate-name" ]; then
        crate="$a"
        break
    fi
    prev="$a"
done

attempt=0
while :; do
    attempt=$((attempt + 1))
    out="$(mktemp)"
    err="$(mktemp)"

    "$@" >"$out" 2>"$err"
    rc=$?

    if [ "$rc" -eq 0 ]; then
        cat "$out"
        cat "$err" >&2
        rm -f "$out" "$err"
        exit 0
    fi

    # Classify: is this a crash/ICE (retryable) or a real error (fail fast)?
    crashed=0
    case "$rc" in
        # 128 + signal: SIGILL=132, SIGABRT=134, SIGBUS=135, SIGFPE=136, SIGSEGV=139
        132 | 134 | 135 | 136 | 139) crashed=1 ;;
    esac
    # rustc reports an ICE with exit 101 (same as a normal compile error), so
    # distinguish by the panic/ICE signature in stderr.
    if [ "$rc" -eq 101 ] && grep -qiE \
        'internal compiler error|unexpectedly panicked|error: rustc interrupted by SIG|signal: (4|6|7|8|11)' \
        "$err"; then
        crashed=1
    fi

    if [ "$crashed" -eq 1 ] && [ "$attempt" -lt "$max" ]; then
        echo "nros-rustc-retry: rustc crashed (rc=$rc) compiling '$crate', retry $attempt/$((max - 1))" >&2
        rm -f "$out" "$err"
        continue
    fi

    # Out of retries, or a real compile error — forward this attempt verbatim.
    cat "$out"
    cat "$err" >&2
    rm -f "$out" "$err"
    exit "$rc"
done
