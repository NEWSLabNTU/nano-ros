#!/bin/bash
# Debug script to capture and analyze liveliness tokens

set -e

PROJECT_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TALKER="$PROJECT_ROOT/target/release/talker"
Z_SUB="$PROJECT_ROOT/packages/rmw/zenoh/zpico-sys/zenoh-pico/build/examples/z_sub"

echo "=== Liveliness Token Debug ==="
echo ""

# Ensure a router is running. Issue 0654 — the vendored `zenohd` is retired
# (phase-362); the router is ROS's `rmw_zenohd`, which takes NO command-line
# configuration: it does not parse argv, so `--listen` is not rejected, it is
# UNREAD, and the router silently comes up on the default port. `nros_router_exec`
# is the one spelling that resolves the binary and passes the locator by
# environment; it `exec`s, so it runs in a subshell here.
. "$PROJECT_ROOT/scripts/dev/zenohd.sh"
ROUTER_PID=""
if ! pgrep -x rmw_zenohd > /dev/null; then
    echo "Starting rmw_zenohd..."
    ( nros_router_exec "${ZENOH_LOCATOR:-tcp/127.0.0.1:7447}" ) &
    ROUTER_PID=$!
    # Kill by PID, never `pkill -f`: the pattern matches the shell running it.
    trap '[ -n "$ROUTER_PID" ] && kill "$ROUTER_PID" 2>/dev/null' EXIT
    sleep 2
fi

echo "Subscribing to liveliness tokens (@ros2_lv/**)..."
echo "Will show tokens from nros and ROS 2 nodes"
echo ""
echo "In another terminal, run:"
echo "  $TALKER --tcp 127.0.0.1:7447"
echo ""
echo "Press Ctrl+C to stop"
echo ""

# Subscribe to all liveliness tokens
"$Z_SUB" -m client -e ${ZENOH_LOCATOR:-tcp/127.0.0.1:7447} -k "@ros2_lv/**" 2>&1
