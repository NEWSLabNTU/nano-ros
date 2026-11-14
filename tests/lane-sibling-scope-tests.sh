#!/usr/bin/env bash
# `nros_lane_sibling_out_of_scope`: a module that owns two fixture platforms
# skips the stage of the one the run did not select. When the run selects the
# OTHER one, that is the run's scope and the lane flushes clean (exit 0). When
# it selects neither, it is still a lane SKIP, and the flush exits 78: the
# negative control, so this cannot pass by never ledgering anything.
set -uo pipefail
cd "$(dirname "$0")/.."
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

fail=0
# <name> <coords-file-content> <expected-flush-rc>
case_() {
    local name=$1 coords=$2 want=$3 rc=0 out
    printf '%b' "$coords" > "$tmp/coords"
    out="$(NROS_BUILD_ROOT="$tmp/build" NROS_FIXTURE_COORDS="$tmp/coords" bash -c '
        source scripts/build/lane-skip.sh
        source scripts/build/fixture-lane.sh
        nros_lane_skip_reset nuttx
        if ! nros_lane_wants_platform nuttx; then
            nros_lane_sibling_out_of_scope nuttx "the arm kernel: no nuttx (arm) coordinate in this run" nuttx-riscv || exit $?
        fi
        nros_lane_skip_flush nuttx "NuttX fixtures built."
    ' 2>&1)" || rc=$?
    if [ "$rc" -eq "$want" ]; then
        echo "  ok   $name (rc $rc)"
    else
        echo "  FAIL $name: want rc $want, got $rc"; printf '%s\n' "$out" | sed 's/^/       /'; fail=1
    fi
}

case_ "sibling selected: run scope, flush clean" "nuttx-riscv,c,zenoh\n" 0
case_ "both selected: nothing skipped" "nuttx,c,zenoh
nuttx-riscv,c,zenoh
" 0
case_ "negative control: neither selected, still SKIPPED" "freertos,c,zenoh
" 78

# The sibling form must not write the ledger at all.
printf 'nuttx-riscv,c,zenoh\n' > "$tmp/coords"
NROS_BUILD_ROOT="$tmp/build2" NROS_FIXTURE_COORDS="$tmp/coords" bash -c '
    source scripts/build/lane-skip.sh; source scripts/build/fixture-lane.sh
    nros_lane_skip_reset nuttx
    nros_lane_sibling_out_of_scope nuttx "x" nuttx-riscv >/dev/null
    f="$(_nros_lane_skip_file nuttx)"; [ ! -s "$f" ]' \
    && echo "  ok   sibling form leaves the ledger empty" \
    || { echo "  FAIL sibling form wrote the ledger"; fail=1; }

[ "$fail" -eq 0 ] || { echo "lane-sibling-scope: FAILED" >&2; exit 1; }
echo "lane-sibling-scope: OK (4 cases)"
