#!/usr/bin/env bash
# Phase 117.12.B — native (host) E2E vs stock `rmw_cyclonedds_cpp`
# (services). `native` is the ROLE here; the REACH is LINUX, not POSIX:
# `e2e_iface` below shells out to iproute2's `ip -o -br link show`, and the
# sourced `ros2_e2e_common.sh` reads `/proc/net/udp`. One sub-case for now:
#
#   1. nano-ros service server ↔ `ros2 service call` (stock client)
#
# Skips cleanly with [SKIPPED] (exit 0) when /opt/ros/humble or
# rmw_cyclonedds_cpp aren't on PATH.

set -u

if [ -z "${NROS_RMW_CYCLONEDDS_SRV_SERVER_BIN:-}" ]; then
    echo "[SKIPPED] NROS_RMW_CYCLONEDDS_SRV_SERVER_BIN not set"
    exit 0
fi
if ! [ -x "$NROS_RMW_CYCLONEDDS_SRV_SERVER_BIN" ]; then
    echo "[SKIPPED] server binary not built"
    exit 0
fi

ROS_SETUP="${ROS_SETUP:-/opt/ros/humble/setup.bash}"
if ! [ -f "$ROS_SETUP" ]; then
    echo "[SKIPPED] $ROS_SETUP not found"
    exit 0
fi

set +u
# shellcheck disable=SC1090
. "$ROS_SETUP"
set -u

if ! command -v ros2 >/dev/null 2>&1; then
    echo "[SKIPPED] ros2 CLI not on PATH after sourcing"
    exit 0
fi

export RMW_IMPLEMENTATION=rmw_cyclonedds_cpp
# issue 0580 — a per-run domain, not a literal: a fixed one is a shared bus
# that two concurrent copies of this test discover each other on.
# shellcheck source=packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/ros2_e2e_common.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/ros2_e2e_common.sh"
export ROS_DOMAIN_ID="${ROS_DOMAIN_ID:-$(nros_unique_ros_domain_id)}"

e2e_iface() {
    if [ -n "${NROS_RMW_CYCLONEDDS_E2E_IFACE:-}" ]; then
        printf '%s\n' "$NROS_RMW_CYCLONEDDS_E2E_IFACE"; return 0
    fi
    ip -o -br link show | awk '
        /BROADCAST,MULTICAST/ && / UP / &&
        $1 !~ /^(docker|veth|tap|qemu|tailscale)/ { print $1; exit }'
}
IFACE=$(e2e_iface)
if [ -z "$IFACE" ]; then
    echo "[SKIPPED] no multicast-capable ethernet interface for SPDP"
    exit 0
fi

CYCLONE_XML=$(mktemp --suffix=.xml)
SERVER_OUT=$(mktemp)
CALL_OUT=$(mktemp)
trap 'rm -f "$CYCLONE_XML" "$SERVER_OUT" "$CALL_OUT"' EXIT
cat > "$CYCLONE_XML" <<XML
<?xml version="1.0" encoding="UTF-8" ?>
<CycloneDDS xmlns="https://cdds.io/config">
  <Domain id="any">
    <General>
      <Interfaces>
        <NetworkInterface name="$IFACE" priority="default" multicast="default" />
      </Interfaces>
    </General>
  </Domain>
</CycloneDDS>
XML
export CYCLONEDDS_URI="file://$CYCLONE_XML"
echo "  using interface=$IFACE domain=$ROS_DOMAIN_ID"

# Drop the ros2 daemon so successive test runs don't reuse stale
# discovery state.
ros2 daemon stop >/dev/null 2>&1 || true

NROS_LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}"
ROS_LD_LIBRARY_PATH="${LD_LIBRARY_PATH#*build/install/lib:}"

failed=0

echo "=== 117.12.B.1: ros2 service call → nros server ==="

# Start the nano-ros server first.
env LD_LIBRARY_PATH="$NROS_LD_LIBRARY_PATH" \
    "$NROS_RMW_CYCLONEDDS_SRV_SERVER_BIN" > "$SERVER_OUT" 2>&1 &
SRV_PID=$!

# Give the server time to advertise its request reader + response
# writer before the client starts.
sleep 2

# `ros2 service call` exits 0 once it gets a reply or its internal
# timeout fires. Cap with `timeout` to bound runaway.
timeout 15 env LD_LIBRARY_PATH="$ROS_LD_LIBRARY_PATH" \
    ros2 service call /add_two_ints \
        example_interfaces/srv/AddTwoInts \
        '{a: 7, b: 35}' \
    > "$CALL_OUT" 2>&1
CALL_RC=$?

# Server should reply + exit on its own. Give it a moment.
wait $SRV_PID
SRV_RC=$?

if [ "$SRV_RC" -ne 0 ]; then
    echo "  FAIL: nros server exited rc=$SRV_RC"
    sed 's/^/    /' "$SERVER_OUT"
    failed=$((failed + 1))
elif [ "$CALL_RC" -ne 0 ]; then
    echo "  FAIL: ros2 service call exited rc=$CALL_RC"
    sed 's/^/    /' "$CALL_OUT"
    failed=$((failed + 1))
elif ! grep -q 'sum=42' <(sed 's/sum: */sum=/' "$CALL_OUT"); then
    echo "  FAIL: reply did not contain 'sum=42'"
    sed 's/^/    /' "$CALL_OUT"
    failed=$((failed + 1))
elif ! grep -q '^REPLIED ' "$SERVER_OUT"; then
    echo "  FAIL: server did not log REPLIED"
    sed 's/^/    /' "$SERVER_OUT"
    failed=$((failed + 1))
else
    echo "  PASS: ros2 service call /add_two_ints → sum=42"
fi

# ---------------------------------------------------------------
# Case 2: nano-ros service client → stock ROS 2 server
# ---------------------------------------------------------------
if [ -n "${NROS_RMW_CYCLONEDDS_SRV_CLIENT_BIN:-}" ] &&
   [ -x "$NROS_RMW_CYCLONEDDS_SRV_CLIENT_BIN" ] &&
   ros2 pkg list 2>/dev/null | grep -qx demo_nodes_cpp; then
    echo "=== 117.12.B.2: nros client → ros2 stock server ==="
    CLIENT_OUT=$(mktemp)
    SERVER_LOG=$(mktemp)
    # `ros2 run` is a PYTHON launcher that spawns the C++ node as a child, so
    # `kill $DN_PID` kills the launcher and REPARENTS the node to init. Measured
    # 2026-08-28 on this host: 71 orphaned
    # `/opt/ros/humble/lib/demo_nodes_cpp/add_two_ints_server`, oldest 10 days,
    # each holding a DDS participant — the shape issue 0659 describes and the
    # reason it says "`ros2` (python) does not [take its child down]".
    #
    # This script spawns its peer directly rather than through `nros-tests`'
    # `ManagedProcess`, so it inherits none of that harness's PDEATHSIG + group
    # ledger machinery and has to do the equivalent itself. `setsid` makes the
    # launcher a session/group leader (pgid == its own pid, verified on this
    # host), so the negative-pid kill below takes the whole tree with it.
    setsid env LD_LIBRARY_PATH="$ROS_LD_LIBRARY_PATH" \
        ros2 run demo_nodes_cpp add_two_ints_server > "$SERVER_LOG" 2>&1 &
    DN_PID=$!
    # Reap on ANY exit, not only the happy path. The kill used to sit after the
    # client run, so an early return, a failed assertion or an interrupt between
    # the two leaked the peer — which is most of how 71 accumulated.
    trap 'kill -TERM -"$DN_PID" 2>/dev/null || true;
          rm -f "$CYCLONE_XML" "$SERVER_OUT" "$CALL_OUT" "$CLIENT_OUT" "$SERVER_LOG"' EXIT

    sleep 2

    timeout 15 env LD_LIBRARY_PATH="$NROS_LD_LIBRARY_PATH" \
        "$NROS_RMW_CYCLONEDDS_SRV_CLIENT_BIN" > "$CLIENT_OUT" 2>&1
    CLI_RC=$?

    # Group kill: the launcher's whole session, not just the launcher.
    kill -TERM -"$DN_PID" 2>/dev/null || true
    wait $DN_PID 2>/dev/null || true

    if [ "$CLI_RC" -ne 0 ]; then
        echo "  FAIL: nros client exited rc=$CLI_RC"
        sed 's/^/    /' "$CLIENT_OUT"
        failed=$((failed + 1))
    elif ! grep -q '^SUM=42$' "$CLIENT_OUT"; then
        echo "  FAIL: client did not print 'SUM=42'"
        sed 's/^/    /' "$CLIENT_OUT"
        failed=$((failed + 1))
    else
        echo "  PASS: nros client got SUM=42 from stock server"
    fi
else
    echo "  SKIP: 117.12.B.2 (demo_nodes_cpp or nros client binary missing)"
fi

if [ "$failed" -gt 0 ]; then
    echo "FAIL: $failed sub-case(s) failed"
    exit 1
fi
echo "OK"
exit 0
