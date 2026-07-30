#!/usr/bin/env bash
# Phase 117.12.A — POSIX E2E vs stock `rmw_cyclonedds_cpp`
# (pub/sub). Two sub-cases:
#
#   1. nano-ros publisher → `ros2 topic echo` (consumer)
#   2. `ros2 topic pub` (producer) → nano-ros subscriber
#
# Skips cleanly with [SKIPPED] (exit 0) when /opt/ros/humble or
# rmw_cyclonedds_cpp aren't on PATH.
#
# Required env (set by the CTest harness):
#   NROS_RMW_CYCLONEDDS_PUB_BIN   absolute path to ros2_pub binary
#   NROS_RMW_CYCLONEDDS_SUB_BIN   absolute path to ros2_sub binary
#   LD_LIBRARY_PATH               must put build/install/lib first

set -u

if [ -z "${NROS_RMW_CYCLONEDDS_PUB_BIN:-}" ] ||
   [ -z "${NROS_RMW_CYCLONEDDS_SUB_BIN:-}" ]; then
    echo "[SKIPPED] NROS_RMW_CYCLONEDDS_{PUB,SUB}_BIN not set"
    exit 0
fi

if ! [ -x "$NROS_RMW_CYCLONEDDS_PUB_BIN" ] ||
   ! [ -x "$NROS_RMW_CYCLONEDDS_SUB_BIN" ]; then
    echo "[SKIPPED] pub/sub binaries not built"
    exit 0
fi

ROS_SETUP="${ROS_SETUP:-/opt/ros/humble/setup.bash}"
if ! [ -f "$ROS_SETUP" ]; then
    echo "[SKIPPED] $ROS_SETUP not found"
    exit 0
fi

# Source ROS in a subshell to avoid polluting our env. ROS 2's
# setup.bash trips on `set -u`, so disable nounset for the source
# step and restore afterwards.
set +u
# shellcheck disable=SC1090
. "$ROS_SETUP"
set -u

if ! command -v ros2 >/dev/null 2>&1; then
    echo "[SKIPPED] ros2 CLI not on PATH after sourcing"
    exit 0
fi

export RMW_IMPLEMENTATION=rmw_cyclonedds_cpp
export ROS_DOMAIN_ID="${ROS_DOMAIN_ID:-117}"

# Pick a multicast-capable interface for SPDP. `lo` is not multicast
# capable on Linux, so loopback-only tests need a real ethernet
# interface. If none, skip cleanly. Override via
# NROS_RMW_CYCLONEDDS_E2E_IFACE if the auto-pick is wrong.
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
trap 'rm -f "$CYCLONE_XML"' EXIT
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
# topic-discovery state. Failing here is fine — daemon may not be
# running yet.
ros2 daemon stop >/dev/null 2>&1 || true

# Save the test harness's LD_LIBRARY_PATH (which puts the in-tree
# `build/install/lib` first so our test binaries pick up the
# pinned `libddsc.so.0.10.5`). ros2's Python loader is fragile
# about additional .so paths — strip ours when invoking the CLI
# but restore for our nano-ros binaries.
NROS_LD_LIBRARY_PATH="${LD_LIBRARY_PATH:-}"
ROS_LD_LIBRARY_PATH="${LD_LIBRARY_PATH#*build/install/lib:}"
ros2_run() { LD_LIBRARY_PATH="$ROS_LD_LIBRARY_PATH" ros2 "$@"; }
nros_run() { LD_LIBRARY_PATH="$NROS_LD_LIBRARY_PATH" "$@"; }

failed=0

# ---------------------------------------------------------------
# Case 1: nano-ros publisher → ros2 topic echo
# ---------------------------------------------------------------
echo "=== 117.12.A.1: nros pub → ros2 echo ==="

# Start ros2 echo in the background so it's listening before the
# publisher starts. `--once` would be ideal but isn't reliable
# pre-Iron — use a finite duration and let timeout kill it.
ECHO_OUT=$(mktemp)
trap 'rm -f "$ECHO_OUT"' EXIT

# Start the publisher first so its writer + topic discovery is
# already announced when ros2 echo's subscriber comes up. Pub runs
# 50 × 100 ms = 5 s of samples.
env LD_LIBRARY_PATH="$NROS_LD_LIBRARY_PATH" \
    "$NROS_RMW_CYCLONEDDS_PUB_BIN" >/dev/null 2>&1 &
PUB_PID=$!

# Give the writer a chance to enter discovery before the reader
# starts (avoids late-joiner data loss with VOLATILE durability).
sleep 1

timeout 8 env LD_LIBRARY_PATH="$ROS_LD_LIBRARY_PATH" \
    ros2 topic echo --csv /chatter std_msgs/msg/String > "$ECHO_OUT" 2>/dev/null &
ECHO_PID=$!

wait $ECHO_PID 2>/dev/null
wait $PUB_PID  2>/dev/null

if grep -qx 'hello-from-nros' "$ECHO_OUT"; then
    echo "  PASS: ros2 echo captured 'hello-from-nros'"
else
    echo "  FAIL: ros2 echo did not capture expected payload"
    echo "    captured ($(wc -l < "$ECHO_OUT") line(s)):"
    sed 's/^/      /' "$ECHO_OUT" || true
    failed=$((failed + 1))
fi

# ---------------------------------------------------------------
# Case 2: ros2 topic pub → nano-ros subscriber
# ---------------------------------------------------------------
echo "=== 117.12.A.2: ros2 pub → nros sub ==="

SUB_OUT=$(mktemp)
trap 'rm -f "$ECHO_OUT" "$SUB_OUT"' EXIT

# Start nano-ros subscriber first so it's matched when ros2 pub fires.
env LD_LIBRARY_PATH="$NROS_LD_LIBRARY_PATH" \
    "$NROS_RMW_CYCLONEDDS_SUB_BIN" > "$SUB_OUT" 2>/dev/null &
SUB_PID=$!

sleep 1

# `ros2 topic pub` repeating at 5 Hz so the subscriber wins the
# discovery race even on a cold cache.
env LD_LIBRARY_PATH="$ROS_LD_LIBRARY_PATH" \
    ros2 topic pub -r 5 /chatter std_msgs/msg/String '{data: hello-from-ros2}' \
    >/dev/null 2>&1 &
ROS_PUB_PID=$!

# Wait for subscriber to print + exit.
if wait $SUB_PID; then
    if grep -qx 'DATA=hello-from-ros2' "$SUB_OUT"; then
        echo "  PASS: nros sub captured 'hello-from-ros2'"
    else
        echo "  FAIL: nros sub captured unexpected payload:"
        sed 's/^/    /' "$SUB_OUT" || true
        failed=$((failed + 1))
    fi
else
    echo "  FAIL: nros sub exited non-zero or timed out"
    sed 's/^/    /' "$SUB_OUT" || true
    failed=$((failed + 1))
fi
kill $ROS_PUB_PID 2>/dev/null || true
wait $ROS_PUB_PID 2>/dev/null || true

if [ "$failed" -gt 0 ]; then
    echo "FAIL: $failed sub-case(s) failed"
    exit 1
fi
echo "OK"
exit 0
