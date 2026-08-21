#!/usr/bin/env bash
# measure-tier-priority-cell.sh — issue 0636.
#
# Usage: scripts/dev/measure-tier-priority-cell.sh [runs] [spinners] [cell]
#
#   cell    the cell label as the test prints it, e.g. `nuttx rust` (default),
#           `nuttx cpp`, `freertos cpp`, `freertos c`.
#
# Runs the `TierPriority/<cell>` cell N times and reports the pass rate with the
# 1-min load recorded per run. `spinners` (default 0) pins that many busy loops
# first, because EVERY failing run in issue 0636's history shared the host with
# a build, and CLAUDE.md records that full-sweep QEMU lanes flake under load: a
# quiet-host pass does not rule the starvation out, it may only mask it. It also
# stresses the right thing for the residue's actual cause — a harness SHORT READ
# (ce152db1f) is a buffering race, and buffer timing moves with load.
#
# Reads the cell's OWN `A/B tiers ACCEPT` line rather than the aggregate test
# result — `sched_dims_applied` is one test over many cells, and a SKIPPED cell
# would otherwise read as a pass (the absorbing-verdict trap, issue 0445). B is
# per-cell (the bringup's tier count, not the seam's), so it is read from the
# line instead of being asserted here: freertos-cpp declares 3, freertos-c and
# both nuttx arms declare 2. A PARTIAL line (`2/3`) is a FAIL — matching only
# "tiers ACCEPT" would score it as a pass, which is the same
# one-tier-satisfies-the-image mistake phase-358 W4 fixed in the assert itself.
#
# The rates this issue accumulated before it existed went 1/5 -> 3/5 -> 4/6 ->
# "16/20" and were each read as progress; the corrected reading was that the
# batch spread exceeded the effect. Use enough runs (30+) to say anything.
cd "$(dirname "$0")/../.."   # one author's home is not a path other checkouts have
source ./activate.sh >/dev/null 2>&1
N="${1:-20}"
HOGS="${2:-0}"
CELL="${3:-nuttx rust}"
if [ "$HOGS" -gt 0 ]; then
    for _ in $(seq 1 "$HOGS"); do (while :; do :; done) & done
    # shellcheck disable=SC2046
    trap 'kill $(jobs -p) 2>/dev/null' EXIT
    echo "induced load: $HOGS spinner(s) on $(nproc) cores"
fi
echo "cell: [$CELL TierPriority]"
pass=0; fail=0
for i in $(seq 1 "$N"); do
    load=$(awk '{print $1}' /proc/loadavg)
    out=$(timeout 300 cargo nextest run -p nros-tests --test sched_dims_applied_e2e \
            --no-fail-fast --success-output final 2>&1)
    # Fork-free match (issue 0726). A `grep -q` here cannot tell a NON-MATCH
    # from a matcher that failed to START, and in a measurement loop the second
    # is recorded as a FAILED RUN — a false negative that corrupts the very
    # rate this script exists to establish, in the direction that looks like
    # the bug still being present. `case` on the captured output forks nothing.
    _hit=0
    while IFS= read -r line; do
        case "$line" in
          *"[$CELL TierPriority] "*"tiers ACCEPT"*) ;;
          *) continue ;;
        esac
        # `…] A/B tiers ACCEPT` — take the field after the bracket, split on `/`.
        counts="${line#*"[$CELL TierPriority] "}"
        counts="${counts%% *}"
        got="${counts%%/*}"; want="${counts##*/}"
        if [ -n "$want" ] && [ "$got" = "$want" ]; then _hit=1; fi
        break
    done <<EOF
$out
EOF
    if [ "$_hit" = 1 ]; then
        pass=$((pass+1)); verdict=PASS
    else
        fail=$((fail+1)); verdict=FAIL
        # Diagnostic only — a failure here costs context, not a verdict.
        printf '%s\n' "$out" | grep -iE "$CELL TierPriority|tier .low.|tier .mid.|tier .high." | head -4 >&2 || true
    fi
    echo "run $i: $verdict (load $load)"
done
echo "RESULT: $pass/$N pass, $fail fail"
