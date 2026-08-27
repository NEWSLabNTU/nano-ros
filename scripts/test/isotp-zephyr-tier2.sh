#!/usr/bin/env bash
# RFC-0083 / phase-394 Tier 2 — a Zephyr native_sim image and a Linux zenoh-rs
# peer sharing a host vcan0 over ISO-TP. The Zephyr side's ONLY link is the bus.
#
#   ZEPHYR_IMAGE=<build>/zephyr/zephyr.exe ./scripts/test/isotp-zephyr-tier2.sh
#
# Build the image with CONF_FILE including cmake/zephyr/native-sim-line-3.7.conf,
# cmake/zephyr/posix-sysconf-minimal-libc.conf and
# cmake/zephyr/native-sim-can-host.conf, plus
# -DDTC_OVERLAY_FILE=cmake/zephyr/native-sim-can-host.overlay and
# CONFIG_NROS_ZENOH_LOCATOR="isotp/can#tx_id=0x200;rx_id=0x201".
#
# The locator address is the ZEPHYR DEVICE name from the devicetree ("can"),
# NOT the host interface -- the overlay is what maps that device onto vcan0.
# Getting that wrong fails at session open with no frames and no clue.
set -o pipefail
ZIMAGE=${ZEPHYR_IMAGE:?set ZEPHYR_IMAGE to a built native_sim zephyr.exe}
RS=${ZENOH_RS_EXAMPLES:-$HOME/repos/zenoh/target/release/examples}
OUT=$(mktemp -d); rm -rf "$OUT"; mkdir -p "$OUT"
PIDS=()
cleanup() { for p in "${PIDS[@]}"; do kill -9 "$p" 2>/dev/null; done; }
trap cleanup EXIT

stdbuf -o0 candump -ta vcan0 >"$OUT/dump.log" 2>&1 & PIDS+=($!)
stdbuf -o0 -e0 "$RS/z_sub" -m peer -l 'isotp/vcan0#tx_id=0x201;rx_id=0x200' \
    --no-multicast-scouting -k '**' >"$OUT/sub.log" 2>&1 & PIDS+=($!)
sleep 3
# --seed: native_sim's entropy is deterministic, so two instances would produce
# the same zenoh session id. One instance here, but keep the habit.
timeout 30 stdbuf -o0 -e0 "$ZIMAGE" --seed=4242 >"$OUT/zephyr.log" 2>&1
cleanup; sleep 1
echo "=== zephyr native_sim ==="; tail -12 "$OUT/zephyr.log"
echo "=== zenoh-rs subscriber ==="; grep -c "chatter" "$OUT/sub.log" 2>/dev/null || echo 0
grep -m 3 "Received\|chatter" "$OUT/sub.log" 2>/dev/null | head -3
echo "=== bus ==="
echo "frames: $(wc -l <"$OUT/dump.log")  FF: $(grep -cE '\[8\]  1[0-9A-F] ' "$OUT/dump.log" || true)  FC: $(grep -cE '\[3\]  3[0-9A-F] ' "$OUT/dump.log" || true)"

# The verdict is delivery, not frame count: frames on the bus only prove the
# Zephyr side transmitted, and this link spent a day looking like it had.
n=$(grep -c "Received PUT" "$OUT/sub.log" 2>/dev/null || true)
if [ "${n:-0}" -gt 0 ]; then
    echo "[tier2] PASS: $n /chatter messages Zephyr -> CAN -> Linux zenoh peer"
    rm -rf "$OUT"; exit 0
fi
echo "[tier2] FAIL: the Linux peer received nothing. Logs kept in $OUT"
exit 1
