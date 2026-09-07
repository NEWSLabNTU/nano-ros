#!/usr/bin/env bash
# Phase 117.12.A — native (host) E2E vs stock `rmw_cyclonedds_cpp`
# (pub/sub). `native` is the ROLE here; the REACH is LINUX, not POSIX: the
# sourced `ros2_e2e_common.sh` reads `/proc/net/udp` and its `nros_e2e_iface`
# shells out to iproute2's `ip -o -br link show`. Two sub-cases:
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
# issue 0580 — a per-run domain, not a literal: a fixed one is a shared bus
# that two concurrent copies of this test discover each other on.
# shellcheck source=packages/rmw/cyclonedds/nros-rmw-cyclonedds/tests/ros2_e2e_common.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/ros2_e2e_common.sh"
export ROS_DOMAIN_ID="${ROS_DOMAIN_ID:-$(nros_unique_ros_domain_id)}"

# issue 1139 / issue 1009 — the bus is confined to loopback by the shared
# helper in `ros2_e2e_common.sh`, not by a config spelled out here. This script
# used to write its own, pinned to a real ethernet interface with multicast on,
# which is the LAN: any DDS participant a colleague, a robot or a CI runner
# leaves running was a peer of this cell.
CYCLONE_XML=$(mktemp --suffix=.xml)
ECHO_OUT=$(mktemp)
ECHO_ERR=$(mktemp)
PUB_ERR=$(mktemp)
SUB_OUT=$(mktemp)
SUB_ERR=$(mktemp)
ROS_PUB_ERR=$(mktemp)
ROS_PUB_PID=""
ECHO_PID=""
cleanup() {
    [ -n "$ECHO_PID" ] && kill "$ECHO_PID" 2>/dev/null
    [ -n "$ROS_PUB_PID" ] && kill "$ROS_PUB_PID" 2>/dev/null
    rm -f "$CYCLONE_XML" "$ECHO_OUT" "$ECHO_ERR" "$PUB_ERR" \
          "$SUB_OUT" "$SUB_ERR" "$ROS_PUB_ERR"
}
trap cleanup EXIT

nros_export_cyclone_config "$CYCLONE_XML" || exit 0
echo "  domain=$ROS_DOMAIN_ID"

# issue 1139 — a DEADLINE, not a window.
#
# Every timing constant this script used to carry was a bet that a fixed
# number of seconds covers the startup of a Python `ros2` CLI: a 1 s head
# start, an 8 s echo window, against a publisher that only lived ~7 s. On a
# loaded host that startup is not bounded, and when the bet lost, the only
# evidence left in the log was `captured 0 line(s)`. The loops below stop the
# moment the payload arrives, so the happy path is FASTER than the old fixed
# windows; the deadline is only what a contended host is allowed to spend.
#
# The trade is stated, not hidden: a PASSING run went from ~12 s to ~3 s, and a
# genuinely broken cell now takes up to 2 x this instead of failing in ~12 s.
# That is the right direction for a lane whose whole problem is that a slow host
# and a broken backend produce the same red.
NROS_E2E_DEADLINE_S="${NROS_E2E_DEADLINE_S:-60}"

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

# Dump everything this script kept when a sub-case fails. Issue 1139's central
# complaint is that nobody could establish a cause: the publisher's stdout AND
# stderr went to /dev/null, its exit status was discarded by a bare `wait`, and
# `ros2`'s stderr went to /dev/null too — so a publisher that never opened a
# session and a delivery that never happened printed the identical line.
dump_evidence() {
    local f
    for f in "$@"; do
        [ -s "$f" ] || continue
        echo "    --- $f ---"
        sed 's/^/      /' "$f" || true
    done
}

# One spelling of "has the echo captured our payload yet?" — the A.1 loop asks
# it from two places. `grep -q` sits on its OWN line with its status RETURNED
# rather than driving control flow from a compound line, which is the shape
# `check-grep-q-error-conflation` (issue 0726) names as correct.
a1_captured() {
    grep -qx 'hello-from-nros' "$ECHO_OUT" 2>/dev/null
}

# ---------------------------------------------------------------
# Case 1: nano-ros publisher → ros2 topic echo
# ---------------------------------------------------------------
echo "=== 117.12.A.1: nros pub → ros2 echo ==="

# `ros2 topic echo` first and for the whole deadline. The old order (publisher
# first, +1 s, then an 8 s echo) was there so the writer was announced before
# the reader came up; the publisher's own 2 s post-`create_publisher` discovery
# sleep already covers that, and it covers it however long the CLI takes to
# start.
#
# `--no-daemon`: without it `ros2 topic echo` asks `NodeStrategy` for a daemon,
# and the `ros2 daemon stop` above guarantees there is none, so it SPAWNS one
# and waits for it — a second Python process on the startup path of the step
# whose startup cost is the thing under suspicion. It also leaves that daemon
# behind on this run's domain. Same reasoning as issue 1009 direction 2, which
# put `--no-daemon` on `dds_bus_snapshot`'s three sub-commands.
timeout "$NROS_E2E_DEADLINE_S" env LD_LIBRARY_PATH="$ROS_LD_LIBRARY_PATH" \
    ros2 topic echo --no-daemon --csv /chatter std_msgs/msg/String \
    > "$ECHO_OUT" 2>"$ECHO_ERR" &
ECHO_PID=$!

# The publisher lives ~7 s (2 s discovery sleep + 50 × 100 ms). RESTART it
# until the payload is seen or the deadline passes: a subscriber that came up
# late gets another window instead of nothing at all.
a1_deadline=$(( SECONDS + NROS_E2E_DEADLINE_S ))
captured=0
attempts=0
pub_failures=0
PUB_RC=0
while [ "$SECONDS" -lt "$a1_deadline" ]; do
    attempts=$(( attempts + 1 ))
    env LD_LIBRARY_PATH="$NROS_LD_LIBRARY_PATH" \
        "$NROS_RMW_CYCLONEDDS_PUB_BIN" >/dev/null 2>>"$PUB_ERR" &
    PUB_PID=$!
    while kill -0 "$PUB_PID" 2>/dev/null && [ "$SECONDS" -lt "$a1_deadline" ]; do
        if a1_captured; then
            captured=1
            break
        fi
        sleep 0.2
    done
    if [ "$captured" -eq 1 ]; then
        kill "$PUB_PID" 2>/dev/null || true
        wait "$PUB_PID" 2>/dev/null || true
        break
    fi
    wait "$PUB_PID"
    PUB_RC=$?
    if a1_captured; then
        captured=1
        break
    fi
    # A publisher that exits non-zero never reached `publish` at all — session
    # or entity creation failed. Retrying is right (a busy domain port is the
    # documented transient, issue 0747) but it must be VISIBLE, or the retry
    # launders exactly the failure worth reading. Announced ONCE and then
    # counted: a publisher that cannot open a session fails in milliseconds, so
    # printing every attempt buries the cause under its own retries — which is
    # what `ros2_e2e_common.sh`'s negative control (`NROS_E2E_LOOPBACK_ADDRESS`
    # at an unroutable address) produced on its first run: 26 identical lines.
    if [ "$PUB_RC" -ne 0 ]; then
        [ "$pub_failures" -eq 0 ] &&
            echo "  note: publisher attempt $attempts exited rc=$PUB_RC — retrying until the deadline"
        pub_failures=$(( pub_failures + 1 ))
        sleep 0.5
    fi
done
kill "$ECHO_PID" 2>/dev/null || true
wait "$ECHO_PID" 2>/dev/null || true
ECHO_PID=""

if [ "$captured" -eq 1 ]; then
    echo "  PASS: ros2 echo captured 'hello-from-nros' (publisher attempt $attempts)"
else
    echo "  FAIL: ros2 echo did not capture expected payload in ${NROS_E2E_DEADLINE_S}s"
    echo "    publisher attempts: $attempts ($pub_failures exited non-zero), last rc=$PUB_RC"
    echo "    captured ($(wc -l < "$ECHO_OUT") line(s)):"
    sed 's/^/      /' "$ECHO_OUT" || true
    dump_evidence "$PUB_ERR" "$ECHO_ERR"
    failed=$((failed + 1))
fi

# ---------------------------------------------------------------
# Case 2: ros2 topic pub → nano-ros subscriber
# ---------------------------------------------------------------
echo "=== 117.12.A.2: ros2 pub → nros sub ==="

# `ros2 topic pub` first and left running: it repeats at 5 Hz, so a subscriber
# that joins late still gets a sample, and the old `sleep 1` before it bought
# nothing the repeat rate did not already buy.
#
# Issues 0969 / 0970 — the payload is 16 characters ON PURPOSE. With the NUL
# that is a 25-byte CDR message, which is NOT 4-aligned, so the `WIRE=` line
# below can tell the wire bytes (len 25) from a re-encode that pads the payload
# to a 4-byte multiple (len 28, with the pad count in the encapsulation
# options). The previous 'hello-from-ros2' came to exactly 24 and could not
# distinguish the two.
#
# No `--no-daemon` here, unlike `topic echo` above: `ros2 topic pub` does not
# accept the flag in Humble (`error: unrecognized arguments: --no-daemon`) and
# does not need it — it builds a DirectNode rather than asking NodeStrategy for
# a daemon. Measured, not assumed: the flag was added to both, and this half
# failed 6 of 6 subscriber attempts with the publisher never starting.
env LD_LIBRARY_PATH="$ROS_LD_LIBRARY_PATH" \
    ros2 topic pub -r 5 /chatter std_msgs/msg/String \
    '{data: hello-from-ros2!}' >/dev/null 2>"$ROS_PUB_ERR" &
ROS_PUB_PID=$!

# The subscriber's 10 s budget is compiled into `ros2_sub.cpp`, so the script
# cannot widen it — but it can run the binary again. One attempt that expires
# while the Python publisher is still starting IS the A.2 failure the issue
# reports, and a second attempt costs nothing when the first succeeds.
a2_deadline=$(( SECONDS + NROS_E2E_DEADLINE_S ))
sub_ok=0
sub_wrong=0
sub_attempts=0
SUB_RC=0
while [ "$SECONDS" -lt "$a2_deadline" ]; do
    sub_attempts=$(( sub_attempts + 1 ))
    env LD_LIBRARY_PATH="$NROS_LD_LIBRARY_PATH" \
        "$NROS_RMW_CYCLONEDDS_SUB_BIN" > "$SUB_OUT" 2>"$SUB_ERR"
    SUB_RC=$?
    if [ "$SUB_RC" -eq 0 ]; then
        if grep -qx 'DATA=hello-from-ros2!' "$SUB_OUT"; then
            sub_ok=1
        else
            # It TOOK a sample and the sample was not ours. That is a foreign
            # publisher on this topic or a codec fault, and no amount of
            # retrying makes it right — retrying it would launder the one
            # failure here that is a verdict rather than a timeout.
            sub_wrong=1
        fi
        break
    fi
    echo "  note: subscriber attempt $sub_attempts exited rc=$SUB_RC (no sample in its 10s budget)"
done

if [ "$sub_ok" -eq 1 ]; then
    echo "  PASS: nros sub captured 'hello-from-ros2!' (attempt $sub_attempts)"
    # Issues 0969 / 0970 — surface the wire framing the subscriber saw.
    # Reported, not asserted: the framing is the remote peer's plus the
    # RTPS submessage, so pinning an exact number here would be pinning
    # ROS 2's serializer rather than ours. It is printed so a reader of a
    # CI log can see what a real peer actually delivers.
    grep '^WIRE=' "$SUB_OUT" | sed 's/^/    /' || true
elif [ "$sub_wrong" -eq 1 ]; then
    echo "  FAIL: nros sub captured unexpected payload:"
    sed 's/^/    /' "$SUB_OUT" || true
    dump_evidence "$SUB_ERR" "$ROS_PUB_ERR"
    failed=$((failed + 1))
else
    echo "  FAIL: nros sub took no sample in ${NROS_E2E_DEADLINE_S}s ($sub_attempts attempt(s), last rc=$SUB_RC)"
    sed 's/^/    /' "$SUB_OUT" || true
    dump_evidence "$SUB_ERR" "$ROS_PUB_ERR"
    failed=$((failed + 1))
fi
kill "$ROS_PUB_PID" 2>/dev/null || true
wait "$ROS_PUB_PID" 2>/dev/null || true
ROS_PUB_PID=""

if [ "$failed" -gt 0 ]; then
    echo "FAIL: $failed sub-case(s) failed"
    exit 1
fi
echo "OK"
exit 0
