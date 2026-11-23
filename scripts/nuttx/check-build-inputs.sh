#!/usr/bin/env bash
#
# Self-test of `build-nuttx.sh`'s board inputs (issue 2915).
#
# Runs the helpers `build-nuttx.sh` sources (`scripts/nuttx/build-inputs.sh`)
# over the in-tree board defconfigs and a synthetic NuttX `boards/` tree, so it
# needs no NuttX checkout and builds nothing. Each refusal case is the negative
# control for the case beside it that passes (`selftest`, run every time). Then
# checks that `build-nuttx.sh` reaches the helpers. Buildless, <1 s.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
# shellcheck source=scripts/nuttx/build-inputs.sh
. "$ROOT/scripts/nuttx/build-inputs.sh"
# shellcheck source=scripts/lib/grep-q.sh
. "$ROOT/scripts/lib/grep-q.sh"

CFG="packages/boards/nros-board-nuttx-qemu/nuttx-config"
RISCV="$ROOT/$CFG/riscv/defconfig"

tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
nuttx="$tmp/nuttx"
mkdir -p "$nuttx"

fail=0
bad() {
    echo "  FAIL: $*" >&2
    fail=1
}

# expect_ok <label> <want-stdout> <cmd...>
expect_ok() {
    local label="$1" want="$2" out rc=0
    shift 2
    out="$("$@" 2>"$tmp/err")" || rc=$?
    if [ "$rc" -ne 0 ]; then
        bad "$label: refused, wanted '$want': $(cat "$tmp/err")"
    elif [ "$out" != "$want" ]; then
        bad "$label: got '$out', wanted '$want'"
    fi
}

# expect_refused <label> <substring the refusal must name>... -- <cmd...>
expect_refused() {
    local label="$1" rc=0 s
    shift
    local -a must=()
    while [ "$1" != "--" ]; do
        must+=("$1")
        shift
    done
    shift
    "$@" >"$tmp/out" 2>"$tmp/err" || rc=$?
    if [ "$rc" -eq 0 ]; then
        bad "$label: accepted ($(cat "$tmp/out")), wanted a refusal"
        return
    fi
    for s in "${must[@]}"; do
        nros_grep_q -F -- "$s" "$tmp/err" || bad "$label: the refusal does not name '$s': $(cat "$tmp/err")"
    done
}

# The helpers' own cases: each refusal is the negative control for the case
# beside it that passes. Run on every invocation, never behind a flag.
selftest() {
    # --- issue 2915: a relative defconfig is the caller's -------------------------
    expect_ok "relative defconfig from the repo root" "$ROOT/$CFG/riscv/defconfig" \
        nros_nuttx_resolve_defconfig "$CFG/riscv/defconfig" "$ROOT"
    expect_ok "absolute defconfig" "$RISCV" \
        nros_nuttx_resolve_defconfig "$RISCV" "$nuttx"
    # The negative control: the same relative path, from inside the NuttX tree,
    # which is where the script used to read it.
    expect_refused "relative defconfig from elsewhere" "'$CFG/riscv/defconfig'" \
        "$nuttx/$CFG/riscv/defconfig" -- \
        nros_nuttx_resolve_defconfig "$CFG/riscv/defconfig" "$nuttx"
}

selftest

# --- build-nuttx.sh reaches the helpers ---------------------------------------
# awk, which reports "no such line" as an empty result rather than a status.
script="$ROOT/scripts/nuttx/build-nuttx.sh"
first_line() { awk -v re="$1" '$0 ~ re { print NR; exit }' "$script"; }
first_resolve="$(first_line 'nros_nuttx_resolve_defconfig "[$]DEFCONFIG" "[$]PWD"')"
[ -n "$first_resolve" ] \
    || bad "build-nuttx.sh does not resolve DEFCONFIG through nros_nuttx_resolve_defconfig"
# The resolution must come before the first `cd` into the NuttX tree.
first_cd="$(first_line '^[[:space:]]*cd "[$]NUTTX_DIR"')"
if [ -n "$first_resolve" ] && [ -n "$first_cd" ] && [ "$first_resolve" -gt "$first_cd" ]; then
    bad "build-nuttx.sh resolves DEFCONFIG (line $first_resolve) after it enters the NuttX tree (line $first_cd)"
fi

if [ "$fail" -ne 0 ]; then
    echo "check-build-inputs: FAILED (issue 2915)" >&2
    exit 1
fi
echo "check-build-inputs: OK (a relative defconfig is the caller's)"
