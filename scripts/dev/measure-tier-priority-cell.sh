#!/usr/bin/env bash
# measure-tier-priority-cell.sh — issue 0636.
#
# Usage: scripts/dev/measure-tier-priority-cell.sh [runs] [spinners]
#
# Runs the `TierPriority/nuttx/rust` cell N times and reports the pass rate with
# the 1-min load recorded per run. `spinners` (default 0) pins that many busy
# loops first, because EVERY failing run in issue 0636's history shared the host
# with a build, and CLAUDE.md records that full-sweep QEMU lanes flake under
# load: a quiet-host pass does not rule the starvation out, it may only mask it.
#
# Greps the cell's OWN `2/2 tiers ACCEPT` line rather than the aggregate test
# result — `sched_dims_applied` is one test over many cells, and a SKIPPED
# nuttx cell would otherwise read as a pass (the absorbing-verdict trap,
# issue 0445).
#
# The rates this issue accumulated before it existed went 1/5 -> 3/5 -> 4/6 ->
# "16/20" and were each read as progress; the corrected reading was that the
# batch spread exceeded the effect. Use enough runs (30+) to say anything.
cd /home/aeon/repos/nano-ros
source ./activate.sh >/dev/null 2>&1
N="${1:-20}"
HOGS="${2:-0}"
if [ "$HOGS" -gt 0 ]; then
    for _ in $(seq 1 "$HOGS"); do (while :; do :; done) & done
    # shellcheck disable=SC2046
    trap 'kill $(jobs -p) 2>/dev/null' EXIT
    echo "induced load: $HOGS spinner(s) on $(nproc) cores"
fi
pass=0; fail=0
for i in $(seq 1 "$N"); do
    load=$(awk '{print $1}' /proc/loadavg)
    out=$(timeout 300 cargo nextest run -p nros-tests --test sched_dims_applied_e2e \
            --no-fail-fast --success-output final 2>&1)
    if printf '%s' "$out" | grep -q "nuttx rust TierPriority] 2/2 tiers ACCEPT"; then
        pass=$((pass+1)); verdict=PASS
    else
        fail=$((fail+1)); verdict=FAIL
        printf '%s' "$out" | grep -iE "nuttx rust TierPriority|tier .low.|tier .high." | head -4 >&2
    fi
    echo "run $i: $verdict (load $load)"
done
echo "RESULT: $pass/$N pass, $fail fail"
