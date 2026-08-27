#!/usr/bin/env bash
# RFC-0083 / phase-394 W6 — a ROS 2 SERVICE CALL served by a nano-ros node,
# over CAN.
#
#   ZENOHC_ISOTP_LIB=<dir with the isotp libzenohc.so> \
#   NANO_ROS_SERVER=<path to the built service-server> \
#   ./scripts/test/isotp-ros-interop.sh
#
# Build the library with:
#   scripts/can/build-zenohc-can.sh --link isotp --zenoh <fork on a 1.8.0 branch>
#
# Topology:
#
#   ros2 service call  --tcp-->  rmw_zenohd  --ISO-TP over CAN-->  nano-ros node
#
# The router listens on both, so the request crosses the CAN bus and the reply
# comes back the same way. TCP on the CLI side keeps the test about the CAN
# link rather than about rebuilding the ROS CLI.
#
# This is the gate the phase exists for. The multicast CAN link of RFC-0080
# cannot serve it at all: zenoh routes queries to unicast faces only, so a
# service call over that link never reaches a queryable. ISO-TP gives a real
# unicast face.
set -o pipefail

DEV=${CAN_DEV:-vcan0}
LIB=${ZENOHC_ISOTP_LIB:?set ZENOHC_ISOTP_LIB to the directory holding the isotp libzenohc.so}
SERVER=${NANO_ROS_SERVER:?set NANO_ROS_SERVER to the built nano-ros service-server}
ROS_DISTRO_DIR=/opt/ros/${ROS_DISTRO:-humble}
OUT=$(mktemp -d)
PIDS=()

say() { echo "[isotp-ros] $*"; }
die() { echo "[isotp-ros] error: $*" >&2; exit 1; }

kill_all() {
    for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null || true; done
    sleep 1
    for p in "${PIDS[@]}"; do kill -9 "$p" 2>/dev/null || true; done
    PIDS=()
}
trap 'kill_all; rm -rf "$OUT"' EXIT

# Block-buffered stdout is DISCARDED when these are killed rather than allowed
# to exit, which makes a run that worked look like one that produced nothing.
run_bg() { stdbuf -o0 -e0 "$@" & PIDS+=($!); }

# Source ROS here rather than relying on the caller's shell: the harness is run
# from make/just/CI as often as by hand. Note there is deliberately no `set -u`
# anywhere in this script -- ROS's setup.bash dereferences unset variables and
# aborts under it.
if ! command -v ros2 >/dev/null 2>&1; then
    [ -f "$ROS_DISTRO_DIR/setup.bash" ] || die "no ros2 on PATH and no $ROS_DISTRO_DIR/setup.bash"
    # shellcheck disable=SC1090
    . "$ROS_DISTRO_DIR/setup.bash"
    command -v ros2 >/dev/null 2>&1 || die "sourcing $ROS_DISTRO_DIR/setup.bash did not provide ros2"
fi

[ -f "$LIB/libzenohc.so" ] || die "$LIB/libzenohc.so does not exist"
# `strings | grep -q` would report failure on a MATCH: grep -q exits early and
# strings takes SIGPIPE. `grep -c` reads to the end.
[ "$(strings "$LIB/libzenohc.so" | grep -c 'ISO-TP: no such interface' || true)" -gt 0 ] ||
    die "$LIB/libzenohc.so has no ISO-TP link in it"
ip link show "$DEV" >/dev/null 2>&1 || die "$DEV is not up; run scripts/test/vcan-setup.sh"

# The router owns the CAN side. Its identifiers are the mirror image of the
# node's: the router's rx is the node's tx.
ROUTER_EP="isotp/$DEV#tx_id=0x201;rx_id=0x200"
NODE_EP="isotp/$DEV#tx_id=0x200;rx_id=0x201"

sed "s|\"tcp/\[::\]:7447\"|\"tcp/[::]:7447\", \"$ROUTER_EP\"|" \
    "$ROS_DISTRO_DIR/share/rmw_zenoh_cpp/config/DEFAULT_RMW_ZENOH_ROUTER_CONFIG.json5" \
    > "$OUT/router.json5"
grep -c "$DEV" "$OUT/router.json5" >/dev/null || die "failed to inject the CAN endpoint into the router config"
say "router listens on: $(grep -o "isotp/[^\"]*" "$OUT/router.json5")"

# rmw_zenohd and librmw_zenoh_cpp.so name libzenohc.so as a plain DT_NEEDED
# with no RPATH, and the vendored library carries no DT_SONAME, so prepending
# a directory substitutes it wholesale -- no ROS rebuild.
export LD_LIBRARY_PATH="$LIB:${LD_LIBRARY_PATH:-}"
export RMW_IMPLEMENTATION=rmw_zenoh_cpp

say "starting rmw_zenohd"
ZENOH_ROUTER_CONFIG_URI="$OUT/router.json5" \
    run_bg "$ROS_DISTRO_DIR/lib/rmw_zenoh_cpp/rmw_zenohd" >"$OUT/router.log" 2>&1
sleep 5

say "starting the nano-ros service server on $NODE_EP"
NROS_LOCATOR="$NODE_EP" run_bg "$SERVER" >"$OUT/server.log" 2>&1
sleep 6

# Stop any daemon first. A stray `ros2` daemon inherits this environment and
# keeps a session on the bus long after the test -- that has corrupted a peer
# count once already. Humble's `ros2 service call` has no `--no-daemon` flag
# (it is not a universal ros2 option), so stopping it is how you get the same
# guarantee here.
ros2 daemon stop >/dev/null 2>&1 || true

say "calling /add_two_ints with a=20 b=22"
timeout 40 stdbuf -o0 ros2 service call /add_two_ints \
    example_interfaces/srv/AddTwoInts "{a: 20, b: 22}" >"$OUT/call.log" 2>&1
RC=$?

echo "--- ros2 service call ---"; cat "$OUT/call.log"

if [ "$(grep -c 'sum=42' "$OUT/call.log" || true)" -gt 0 ]; then
    ros2 daemon stop >/dev/null 2>&1 || true
    say "PASS: a ROS 2 service call was served by a nano-ros node over CAN"
    exit 0
fi

ros2 daemon stop >/dev/null 2>&1 || true
say "FAIL (ros2 exit $RC)"
echo "--- router ---";  tail -25 "$OUT/router.log"
echo "--- server ---";  tail -25 "$OUT/server.log"
exit 1
