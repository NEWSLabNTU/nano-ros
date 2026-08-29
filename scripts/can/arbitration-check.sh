#!/usr/bin/env bash
# Measure whether zenoh's priority really wins CAN arbitration.
#
#   ./scripts/can/arbitration-check.sh --dev can0 [--bitrate 500000] [--seconds 20]
#
# THIS NEEDS REAL HARDWARE and will refuse a virtual interface. `vcan` queues
# frames rather than contending for them, so every result on it is an artefact
# of scheduling order. That is the whole reason this script exists separately
# from the test suite: arbitration is the one claim the CAN link makes that a
# virtual bus cannot check.
#
# The experiment: saturate the bus with background traffic on a HIGH (losing)
# identifier, then send control-priority traffic on a LOW (winning) identifier,
# and compare the latency of the urgent frames against the same run with the
# priorities swapped. If arbitration is doing what the design claims, the
# urgent stream's tail latency barely moves under load and the bulk stream's
# grows.
set -euo pipefail

DEV=can0
BITRATE=500000
SECONDS_RUN=20
while [ $# -gt 0 ]; do
    case "$1" in
        --dev) DEV="$2"; shift 2 ;;
        --bitrate) BITRATE="$2"; shift 2 ;;
        --seconds) SECONDS_RUN="$2"; shift 2 ;;
        -h|--help) awk 'NR>1 && /^#/ { sub(/^# ?/,""); print; next } NR>1 { exit }' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

# A virtual bus cannot arbitrate. Refuse rather than produce a number that
# looks like a measurement.
if [ -d "/sys/class/net/$DEV" ] && \
   [ "$(cat "/sys/class/net/$DEV/type" 2>/dev/null || echo 0)" = "280" ] && \
   ! [ -d "/sys/class/net/$DEV/device" ]; then
    echo "[arbitration] $DEV has no backing device, so it is virtual." >&2
    echo "[arbitration] vcan queues frames and never contends; this measures nothing there." >&2
    exit 1
fi
command -v cangen >/dev/null || { echo "[arbitration] need can-utils (cangen, candump)" >&2; exit 1; }

echo "[arbitration] $DEV at $BITRATE bit/s, ${SECONDS_RUN}s per direction"
echo "[arbitration] bring the interface up yourself, e.g.:"
echo "    sudo ip link set $DEV type can bitrate $BITRATE && sudo ip link set up $DEV"

run_case() {
    local name=$1 urgent_id=$2 bulk_id=$3 out
    out=$(mktemp -d)
    echo "[arbitration] case: $name  (urgent=0x$urgent_id  bulk=0x$bulk_id)"
    stdbuf -o0 candump -ta "$DEV" > "$out/dump.log" 2>&1 &
    local dump=$!
    # Bulk: saturate. Urgent: one frame every 10ms.
    cangen "$DEV" -I "$bulk_id"   -L 8 -g 0  -n 100000 >/dev/null 2>&1 &
    local bulk=$!
    cangen "$DEV" -I "$urgent_id" -L 8 -g 10 -n $((SECONDS_RUN * 100)) >/dev/null 2>&1
    kill $bulk 2>/dev/null || true
    sleep 0.5
    kill $dump 2>/dev/null || true
    python3 - "$out/dump.log" "$urgent_id" "$name" <<'PY'
import sys, re
path, uid, name = sys.argv[1], sys.argv[2].lower(), sys.argv[3]
ts = [float(m.group(1)) for m in
      (re.match(r'\s*\(([\d.]+)\)\s+\S+\s+' + uid + r'\s', l.lower()) for l in open(path)) if m]
if len(ts) < 10:
    print(f"    {name}: only {len(ts)} urgent frames seen; check wiring"); raise SystemExit
g = sorted((ts[i+1] - ts[i]) * 1000 for i in range(len(ts) - 1))
p50, p99, mx = g[len(g)//2], g[int(len(g)*0.99)], g[-1]
print(f"    {name}: urgent frames {len(ts)}  p50 {p50:.2f} ms  p99 {p99:.2f} ms  max {mx:.2f} ms")
PY
    rm -rf "$out"
}

# Lower identifier wins arbitration on CAN.
run_case "urgent LOW id (should win)"  "020" "700"
run_case "urgent HIGH id (should lose)" "700" "020"

cat <<'NOTE'

[arbitration] Reading the result: the first case is the layout the CAN link
uses, with zenoh's Control priority mapped to the lowest identifiers. If
arbitration is working, its p99 and max stay close to the 10ms send interval
while the second case, where the urgent stream has to wait for the bulk one,
shows a visibly worse tail. If the two cases look the same, the priority
mapping is not buying anything on this hardware and the claim should be
withdrawn.
NOTE
