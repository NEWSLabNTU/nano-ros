#!/bin/bash
# Compare keyexprs used by nros vs ROS 2

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
TALKER="$PROJECT_ROOT/target/release/talker"
Z_SUB="$PROJECT_ROOT/packages/rmw/zenoh/zpico-sys/zenoh-pico/build/examples/z_sub"

# Unified log dir (matches the test fixtures' convention) instead of scattering
# files across /tmp. Override with NROS_TEST_LOG_DIR.
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
LOG_DIR="${NROS_TEST_LOG_DIR:-$REPO_ROOT/test-logs/debug}"
mkdir -p "$LOG_DIR"

# Colors
GREEN='\033[0;32m'
NC='\033[0m'

log_info() { echo -e "${GREEN}[INFO]${NC} $*"; }

# Issue 0654 — the vendored `zenohd` is retired (phase-362). The router is ROS's
# `rmw_zenohd`, which takes NO command-line configuration: argv is not parsed, so
# `--listen` is UNREAD rather than rejected and the router silently binds the
# default port. `nros_router_exec` resolves the binary and passes the locator by
# environment; it `exec`s, hence the subshell.
. "$PROJECT_ROOT/scripts/dev/zenohd.sh"

# Cleanup kills what THIS script started, by PID. `pkill -f <pattern>` matches
# the shell running it as readily as the target — a self-match that silently
# kills the script instead of the peer.
ROUTER_PID=""
TALKER_PID=""
cleanup() {
    [ -n "$ROUTER_PID" ] && kill "$ROUTER_PID" 2>/dev/null
    [ -n "$TALKER_PID" ] && kill "$TALKER_PID" 2>/dev/null
    return 0
}
trap cleanup EXIT

# Start the router
log_info "Starting rmw_zenohd..."
( nros_router_exec "${ZENOH_LOCATOR:-tcp/127.0.0.1:7447}" ) > "$LOG_DIR/zenohd.log" 2>&1 &
ROUTER_PID=$!
sleep 2

echo ""
echo "=== Part 1: nros publisher keyexpr ==="
echo ""

# Start nros talker briefly
timeout 3 "$TALKER" --tcp 127.0.0.1:7447 > "$LOG_DIR/talker.log" 2>&1 &
TALKER_PID=$!
sleep 2

# Subscribe to all data keys (not liveliness) to see what keyexpr is used
log_info "Subscribing to 0/** to capture nros messages..."
timeout 3 "$Z_SUB" -m client -e ${ZENOH_LOCATOR:-tcp/127.0.0.1:7447} -k '0/**' 2>&1 | head -10 || true

# Kill nros talker by the PID we started, not by pattern (see cleanup above).
[ -n "$TALKER_PID" ] && kill "$TALKER_PID" 2>/dev/null
TALKER_PID=""
sleep 1

echo ""
echo "=== Part 2: ROS 2 publisher keyexpr ==="
echo ""

# Source ROS 2 and start talker
source /opt/ros/humble/setup.bash
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
export ZENOH_ROUTER_CHECK_ATTEMPTS=0
export ROS_DOMAIN_ID=0

log_info "Starting ROS 2 talker..."
timeout 5 ros2 run demo_nodes_cpp talker &
ROS2_PID=$!
sleep 2

# Subscribe to all keys to see what keyexpr ROS 2 uses
log_info "Subscribing to 0/** to capture ROS 2 messages..."
timeout 3 "$Z_SUB" -m client -e ${ZENOH_LOCATOR:-tcp/127.0.0.1:7447} -k '0/**' 2>&1 | head -10 || true

kill $ROS2_PID 2>/dev/null || true

echo ""
echo "=== Talker log ==="
cat "$LOG_DIR/talker.log"
