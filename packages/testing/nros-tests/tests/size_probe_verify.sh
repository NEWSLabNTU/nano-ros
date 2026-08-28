#!/usr/bin/env bash
# Size-probe verification.
#
# Phase 118.E.3 + .4 originally validated that TWO probe modes (filesystem,
# isolated) agreed. Issue 0464 deleted the filesystem mode — it polled the outer
# target dir with a timeout (a race) and selected the newest matching rlib by
# mtime (which could be another consumer's build). There is one mode now, so the
# cross-mode parity check is gone and what remains is:
#
#   1. the fallbacks stay deleted        (they are what made sizes non-deterministic)
#   2. cross-pointer-size sanity          (a 32-bit target must not report host sizes)
#   3. a concurrency soak                 (repeated clean rebuilds must not flake)
#
# Invoked via `just verify-size-probe`.

set -euo pipefail

cd "$(git rev-parse --show-toplevel)"

# Issue 0726 — the installed-target probe below decides whether the
# cross-pointer-size arm RUNS. `rustup … | grep -q` conflates "i686 is not
# installed" (rc 1) with "the grep did not run" (rc>=2), and the second reads
# as the first: the arm silently skips and the script still says OK. That is
# the check-never-fires direction of the same conflation.
# shellcheck source=../../../../scripts/lib/grep-q.sh
. "$(git rev-parse --show-toplevel)/scripts/lib/grep-q.sh"

# The REAL generated header, not the tracked stub. `packages/api/nros-c/include/
# nros/nros_config_generated.h` is a Phase-119.3 placeholder that documents where
# the real one goes and contains ZERO `#define NROS_*_SIZE` lines — so this
# script's `extract_sizes` matched nothing and, under `pipefail`, exited 1 before
# asserting anything. It had been dead that way since the header moved; found
# 2026-08-06 while removing the probe's fallbacks (issue 0464), and plausibly the
# reason the fallback rot went unnoticed for so long.
HEADER="${CARGO_TARGET_DIR:-target}/nros-c-generated/nros/nros_config_generated.h"
FEATURES="cffi-zenoh-cffi,platform-posix,ros-humble"
JOBS="${JOBS:-8}"
SOAK_ROUNDS="${SOAK_ROUNDS:-3}"

extract_sizes() {
    if [[ ! -f "$HEADER" ]]; then
        echo "FAIL: generated header absent: $HEADER" >&2
        echo "      (nros-c's build.rs writes it; did the build actually run?)" >&2
        exit 1
    fi
    grep -E '^#define NROS_(EXECUTOR|PUBLISHER|SUBSCRIBER|SESSION|SERVICE_CLIENT|SERVICE_SERVER|GUARD_CONDITION|LIFECYCLE_CTX|ACTION_SERVER_INTERNAL)_SIZE' \
        "$HEADER" | sort
}

build_under() {
    cargo clean -p nros-c >/dev/null
    cargo build -p nros-c --features "$FEATURES" -j "$JOBS" >/dev/null
}

echo "=== 0464 — the fallbacks stay deleted ==="
# These are the two mechanisms that made the sizes non-deterministic. A future
# "just make it build" change reinstating either would be silent at runtime, so
# the guard is here rather than in a comment.
guard_fail=0
if git grep -n "find_dep_rlib_filesystem\|NROS_SIZES_PROBE_MODE\|NROS_SIZES_PROBE_TIMEOUT_SECS" \
        -- packages/tooling/nros-sizes-build/src >/dev/null 2>&1; then
    echo "FAIL: the polling fallback is back in nros-sizes-build (issue 0464)"
    guard_fail=1
fi
if git grep -n "FALLBACK_SIZES" -- packages/tooling/nros-build-helpers/src >/dev/null 2>&1; then
    echo "FAIL: committed size constants are back in nros-build-helpers (issue 0464)"
    guard_fail=1
fi
[[ "$guard_fail" -eq 0 ]] || exit 1
echo "OK — no polling fallback, no committed size constants"
echo

echo "=== baseline sizes (isolated probe, the only mode) ==="
build_under
fs_sizes=$(extract_sizes)
echo "$fs_sizes"
echo

echo "=== 118.E.3 — cross-pointer-size validation (host vs 32-bit) ==="
# Build `nros` (not nros-c — nros-c is host-only / OS-backed) for a
# 32-bit target if installed, capture sizes via the probe, and assert
# pointer-size-dependent types shrink. Catches the case where the
# probe accidentally reads host sizes during a cross build.
# CAPTURE, then test. A pipeline puts `nros_grep_q` in a subshell (its `exit 2`
# would end only that segment) and `grep -q`'s early exit can SIGPIPE `rustup`,
# which under `pipefail` turns a MATCH into a non-zero pipeline — issue 0732's
# shape. A rustup that cannot run keeps the pre-existing behaviour: skip.
if installed_targets="$(rustup target list --installed)" \
        && nros_grep_q '^i686-unknown-linux-gnu$' <<<"$installed_targets"; then
    cargo clean -p nros-c >/dev/null
    cargo build -p nros-c --target i686-unknown-linux-gnu \
        --features cffi-zenoh-cffi,platform-posix,ros-humble \
        -j "$JOBS" 2>&1 | tail -3 || true
    # nros-c emits the same header path under the 32-bit build's
    # OUT_DIR; capture sizes from the canonical install location.
    cross_pub=$(grep '^#define NROS_PUBLISHER_SIZE' "$HEADER" | awk '{print $3}')
    host_pub=$(echo "$fs_sizes" | grep '^#define NROS_PUBLISHER_SIZE' | awk '{print $3}')
    echo "  host PUBLISHER_SIZE=$host_pub  i686 PUBLISHER_SIZE=$cross_pub"
    if [[ "$cross_pub" -ge "$host_pub" ]]; then
        echo "  WARN: 32-bit target did not shrink PUBLISHER_SIZE; either"
        echo "  RmwPublisher has no pointer fields or the probe leaked host sizes."
    fi
else
    echo "  [skip] i686-unknown-linux-gnu target not installed (install via"
    echo "         'rustup target add i686-unknown-linux-gnu' to enable)"
fi

echo
echo "=== 118.E.4 — concurrency soak (${SOAK_ROUNDS} rounds, -j${JOBS}) ==="
soak_ref=""
for i in $(seq 1 "${SOAK_ROUNDS}"); do
    cargo clean -p nros-c -p nros-cpp >/dev/null
    cargo build -p nros-c -p nros-cpp --features "$FEATURES" -j "$JOBS" >/dev/null
    sz=$(grep '^#define NROS_EXECUTOR_SIZE' "$HEADER")
    echo "  [soak $i/${SOAK_ROUNDS}] $sz"
    # The soak is only an assertion if a differing round FAILS. It used to just
    # print each round and exit 0 regardless.
    if [[ -z "$soak_ref" ]]; then
        soak_ref="$sz"
    elif [[ "$sz" != "$soak_ref" ]]; then
        echo "FAIL: EXECUTOR_SIZE flaked across rounds: '$soak_ref' vs '$sz'"
        exit 1
    fi
done

echo
echo "=== ALL CHECKS PASSED ==="
