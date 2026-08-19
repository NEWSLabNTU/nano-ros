# PROBE-OWNED verification, zenoh track (phase-368 follow-up). Appended after
# the book-extracted steps by run-bootstrap-probe.sh; runs in the same shell.
#
# Replays getting-started/ros2-interop.md's Quick Start non-interactively on
# the ros:humble image that page presumes: install the page's own prerequisite
# (`ros-humble-rmw-zenoh-cpp`, its documented apt line), start the router with
# the page's exact invocation, run the nano-ros talker the first-node chapter
# just built, and prove CROSS-STACK DELIVERY with the page's `ros2 topic echo`
# — the one assertion the cyclonedds quickstart track cannot make.

echo '=== probe verify: zenoh ROS-interop runtime ==='
repo_root="$(git rev-parse --show-toplevel)"
cd "$repo_root"
source ./activate.sh

command -v nros >/dev/null || { echo "PROBE FAIL: nros not on PATH after bootstrap+activate"; exit 1; }
nros version

# The interop page's prerequisite line, verbatim in spirit (the probe host is
# root, so no sudo): `sudo apt install ros-humble-rmw-zenoh-cpp`.
apt-get install -y -qq ros-humble-rmw-zenoh-cpp >/dev/null

# Terminal 1 — the router, exactly as the page spells it. `ros2` needs the
# ROS env; keep it scoped to this subshell so the nano-ros build env stays
# clean (the page runs it in its own terminal for the same reason).
(
    source /opt/ros/humble/setup.bash
    ZENOH_CONFIG_OVERRIDE='listen/endpoints=["tcp/127.0.0.1:7447"];scouting/multicast/enabled=false' \
        exec ros2 run rmw_zenoh_cpp rmw_zenohd
) >/tmp/zenohd.log 2>&1 &
router_pid=$!
trap 'kill "$router_pid" 2>/dev/null || true' EXIT

# Terminal 2 — the nano-ros talker (built by the extracted first-node step).
cd examples/native/rust/talker
RUST_LOG=info timeout 120 cargo run >/tmp/talker.log 2>&1 &
talker_pid=$!

pattern="Publishing: 'Hello World: 1'"
deadline=$((SECONDS + 90))
until grep -qF "$pattern" /tmp/talker.log; do
    if ! kill -0 "$talker_pid" 2>/dev/null; then
        echo "PROBE FAIL: talker exited before publishing"
        tail -40 /tmp/talker.log; tail -20 /tmp/zenohd.log
        exit 1
    fi
    if ((SECONDS >= deadline)); then
        echo "PROBE FAIL: no '$pattern' within 90 s"
        tail -40 /tmp/talker.log; tail -20 /tmp/zenohd.log
        exit 1
    fi
    sleep 2
done
echo "PROBE PASS: nano-ros talker published through the documented router"

# Terminal 3 — the page's delivery proof: a stock ROS 2 subscriber receives
# what nano-ros published. best_effort per the page (the talker publishes
# best-effort; a RELIABLE echo silently delivers nothing).
(
    source /opt/ros/humble/setup.bash
    export RMW_IMPLEMENTATION=rmw_zenoh_cpp
    export ZENOH_CONFIG_OVERRIDE='mode="client";connect/endpoints=["tcp/127.0.0.1:7447"]'
    exec timeout 60 ros2 topic echo /chatter std_msgs/msg/String --qos-reliability best_effort
) >/tmp/echo.log 2>&1 &
echo_pid=$!

deadline=$((SECONDS + 60))
until grep -q "data: 'Hello World:" /tmp/echo.log; do
    if ((SECONDS >= deadline)); then
        echo "PROBE FAIL: ros2 topic echo received nothing within 60 s"
        tail -30 /tmp/echo.log; tail -20 /tmp/zenohd.log; tail -20 /tmp/talker.log
        exit 1
    fi
    sleep 2
done
kill "$echo_pid" "$talker_pid" 2>/dev/null || true

echo "PROBE PASS: stock ros2 topic echo received nano-ros's messages — cross-stack delivery proven"
