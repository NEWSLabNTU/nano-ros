#!/usr/bin/env bash
# phase-394 — ROS 2 PARAMETERS on a nano-ros node, over CAN (ISO-TP).
#
# `ros2 param` is SIX services (get / set / list / describe / get_types /
# set_atomically) plus the /parameter_events topic -- the widest service
# surface in ROS, and none of it works over a multicast face.
#
# Topology, and why it differs from the service/action tests:
#
#   ros2 param ... --tcp--> rmw_zenohd --ISO-TP over CAN--> nano-ros node
#
# Each `ros2 param` invocation is a SHORT-LIVED process with its own session.
# Pointing those at the identifier pair directly makes every invocation a new
# listener on it while the node is still reconnecting to the last one, and the
# handshake collides -- "Received invalid message instead of an OpenSyn". A
# router is the persistent end of the pair, which is also how a real deployment
# looks: an MCU on the bus, a router on the vehicle computer. The node's ONLY
# link is still ISO-TP, so the parameter traffic still crosses CAN.
# issue 0726 — `nros_grep_q` exits 2 on a TOOL failure instead of reporting a
# finding; a raw `grep -q` under load once reported a failed-to-fork grep as a
# missing anchor.
. "$(dirname "$0")/../lib/grep-q.sh"
OUT=$(mktemp -d); rm -rf "$OUT"; mkdir -p "$OUT"
. /opt/ros/${ROS_DISTRO:-humble}/setup.bash
export LD_LIBRARY_PATH=${ZENOHC_ISOTP_LIB:?set ZENOHC_ISOTP_LIB}:${LD_LIBRARY_PATH:-}
export RMW_IMPLEMENTATION=rmw_zenoh_cpp

sed 's|"tcp/\[::\]:7447"|"tcp/[::]:7447", "isotp/vcan0#tx_id=0x201;rx_id=0x200"|' \
    /opt/ros/${ROS_DISTRO:-humble}/share/rmw_zenoh_cpp/config/DEFAULT_RMW_ZENOH_ROUTER_CONFIG.json5 \
    > "$OUT/router.json5"
nros_grep_q 'isotp/vcan0' "$OUT/router.json5" || { echo "FAIL: no CAN endpoint injected"; exit 1; }

ros2 daemon stop >/dev/null 2>&1 || true
stdbuf -o0 candump -ta vcan0 >"$OUT/dump.log" 2>&1 &
CD=$!
# Located through ament, not a literal prefix (issues 0653/0654): the
# `/opt/ros/<distro>/lib/...` spelling is the THIRD of the resolver's three
# steps, so it is wrong on a host whose ROS is a colcon overlay. Shell cannot
# reach `nros_zenohd_bin`; `ros2 pkg prefix` asks the same question
# path-independently.
ZENOHD="$(ros2 pkg prefix rmw_zenoh_cpp)/lib/rmw_zenoh_cpp/rmw_zenohd"
ZENOH_ROUTER_CONFIG_URI="$OUT/router.json5" \
  stdbuf -o0 "$ZENOHD" >"$OUT/router.log" 2>&1 &
RTR=$!
sleep 5
# NROS_ENTRY_SPIN_MS=forever: the LAUNCH arm of `nros::main!` runs an env-gated
# BOUNDED spin and then prints "nros: application complete" and exits. Without
# this the node is gone before any `ros2 param` call reaches it, and the only
# symptom is an empty graph -- which reads like a transport failure.
NROS_ENTRY_SPIN_MS=forever \
NROS_LOCATOR='isotp/vcan0#tx_id=0x200;rx_id=0x201' \
  stdbuf -o0 "${NANO_ROS_PARAM_NODE:?set NANO_ROS_PARAM_NODE to the built native_rust_params_entry}" >"$OUT/node.log" 2>&1 &
NODE=$!
sleep 10

echo "=== ros2 node list ==="  ; timeout 30 ros2 node list          2>&1 | tee "$OUT/nodes.log" | tail -5
echo "=== ros2 param list ===" ; timeout 30 ros2 param list /param_talker 2>&1 | tee "$OUT/list.log" | tail -8
echo "=== get (initial) ==="   ; timeout 30 ros2 param get /param_talker publish_period_ms 2>&1 | tee "$OUT/get1.log"
echo "=== set 250 ==="         ; timeout 30 ros2 param set /param_talker publish_period_ms 250 2>&1 | tee "$OUT/set.log"
echo "=== get (after set) ===" ; timeout 30 ros2 param get /param_talker publish_period_ms 2>&1 | tee "$OUT/get2.log"

kill -9 $NODE $RTR $CD 2>/dev/null
ros2 daemon stop >/dev/null 2>&1 || true
echo "=== bus ==="
echo "frames: $(wc -l <"$OUT/dump.log")  FF: $(grep -cE '\[8\]  1[0-9A-F] ' "$OUT/dump.log")  FC: $(grep -cE '\[3\]  3[0-9A-F] ' "$OUT/dump.log")"

# The verdict is the ROUND TRIP, not any single call: read the initial value,
# change it, read it back changed. A `get` alone would pass against a node that
# ignores `set` entirely.
ok=0
nros_grep_q 'Integer value is: 120'     "$OUT/get1.log" && ok=$((ok+1))
nros_grep_q 'Set parameter successful'  "$OUT/set.log"  && ok=$((ok+1))
nros_grep_q 'Integer value is: 250'     "$OUT/get2.log" && ok=$((ok+1))
nros_grep_q 'publish_period_ms'         "$OUT/list.log" && ok=$((ok+1))
if [ "$ok" -eq 4 ]; then
    echo "[isotp-params] PASS: list + get 120 + set 250 + get 250, all over CAN"
    rm -rf "$OUT"; exit 0
fi
echo "[isotp-params] FAIL ($ok/4). Logs kept in $OUT"
exit 1
