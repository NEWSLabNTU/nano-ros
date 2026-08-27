#!/usr/bin/env bash
# RFC-0083 / phase-394 W6 — ROS 2 SERVICE CALLS over CAN, in both roles.
#
#   ZENOHC_ISOTP_LIB=<dir with the isotp libzenohc.so> \
#   NANO_ROS_SERVER=<built nano-ros service-server> \
#   NANO_ROS_CLIENT=<built nano-ros service-client> \
#   NANO_ROS_ACTION_SERVER=<built nano-ros action-server> \
#   NANO_ROS_ACTION_CLIENT=<built nano-ros action-client> \
#   NANO_ROS_ACTION_SERVER_PACED=<action-server built with NROS_FIB_STEP_TICKS=5000> \
#   ./scripts/test/isotp-ros-interop.sh [--role <role>]
#
#   --role service-server | service-client | action-server | action-client
#          action-cancel
#          both (the two service roles, the default) | all (all four + cancel)
#
# Build the library with:
#   scripts/can/build-zenohc-can.sh --link isotp --zenoh <fork on a 1.8.0 branch>
#
# There is NO ROUTER and NO TCP ENDPOINT anywhere in the path. The ROS 2 side
# is a plain rmw_zenoh peer whose session config lists exactly one endpoint --
# the ISO-TP one -- and whose connect list is empty. The CAN bus is the only
# way the two processes can reach each other, so a passing run cannot be
# explained by anything else.
#
# The multicast CAN link of RFC-0080 cannot serve a service call at all: zenoh
# routes queries to unicast faces only, so the request never reaches a
# queryable. That is a property of zenoh's multicast transport, not of CAN.
set -o pipefail

DEV=${CAN_DEV:-vcan0}
LIB=${ZENOHC_ISOTP_LIB:?set ZENOHC_ISOTP_LIB to the directory holding the isotp libzenohc.so}
ROS_DISTRO_DIR=/opt/ros/${ROS_DISTRO:-humble}
ROLE=both
OUT=$(mktemp -d)
PIDS=()
FAILED=0

while [ $# -gt 0 ]; do
    case "$1" in
        --role) ROLE="$2"; shift 2 ;;
        --keep) KEEP=1; shift ;;
        -h | --help) awk 'NR>1 && /^#/ { sub(/^# ?/,""); print; next } NR>1 { exit }' "$0"; exit 0 ;;
        *) echo "unknown option: $1" >&2; exit 2 ;;
    esac
done

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/grep-q.sh
. "$script_dir/../lib/grep-q.sh"

say() { echo "[isotp-ros] $*"; }
die() { echo "[isotp-ros] error: $*" >&2; exit 1; }
# Counting goes through the shared helper: `grep -c … || true` cannot tell
# no-match (prints 0, exits 1) from a tool FAILURE (prints nothing, exits >=2),
# so a log that was never written reads as "the reply never arrived" — this
# script's own failure verdict, produced from missing evidence.
# See scripts/lib/grep-q.sh.

# Kill the process GROUP, resolved from the child with `ps` rather than assumed
# to equal its pid: `setsid` may fork, in which case `$!` is setsid's pid and
# not the session leader's, so `kill -- -$!` hits nothing and a `ros2 run`
# wrapper's child survives. One did, for eleven minutes, and answered a later
# test's flow control.
kill_all() {
    local p pg
    for p in "${PIDS[@]}"; do
        pg=$(ps -o pgid= -p "$p" 2>/dev/null | tr -d ' ')
        [ -n "$pg" ] && kill -- "-$pg" 2>/dev/null
        kill "$p" 2>/dev/null || true
    done
    sleep 1
    for p in "${PIDS[@]}"; do
        pg=$(ps -o pgid= -p "$p" 2>/dev/null | tr -d ' ')
        [ -n "$pg" ] && kill -9 -- "-$pg" 2>/dev/null
        kill -9 "$p" 2>/dev/null || true
    done
    PIDS=()
}
cleanup() {
    kill_all
    ros2 daemon stop >/dev/null 2>&1 || true
    [ -n "${KEEP:-}" ] && { echo "[isotp-ros] logs kept in $OUT"; return; }
    rm -rf "$OUT"
}
trap cleanup EXIT

# Everything below is KILLED rather than allowed to exit, and block-buffered
# stdout is DISCARDED on SIGTERM -- a run that worked then logs nothing, which
# reads exactly like a lost message. It cost a debugging cycle once.
#
# `setsid` + killing the process GROUP, not the pid: `ros2 run` is a wrapper
# that execs the node as a child, so killing the wrapper leaves the node alive
# holding an ISO-TP session on the bus. A survivor from this script sat on
# vcan0 for eleven minutes and answered a LATER, unrelated test's flow control
# -- every frame acknowledged twice, which reads exactly like a bug in the
# implementation under test.
run_bg() { setsid stdbuf -o0 -e0 "$@" & PIDS+=($!); }

# Source ROS here rather than trusting the caller's shell. Note there is
# deliberately no `set -u` anywhere: ROS's setup.bash dereferences unset
# variables and aborts under it.
if ! command -v ros2 >/dev/null 2>&1; then
    [ -f "$ROS_DISTRO_DIR/setup.bash" ] || die "no ros2 on PATH and no $ROS_DISTRO_DIR/setup.bash"
    # shellcheck disable=SC1090
    . "$ROS_DISTRO_DIR/setup.bash"
    command -v ros2 >/dev/null 2>&1 || die "sourcing $ROS_DISTRO_DIR/setup.bash did not provide ros2"
fi

[ -f "$LIB/libzenohc.so" ] || die "$LIB/libzenohc.so does not exist"
# NOT `strings | grep -q`: grep -q exits on the first match, strings takes
# SIGPIPE, and under `pipefail` the pipeline reports failure ON A MATCH.
# Deliberately NOT `nros_grep_count` either: that helper reads files, and here
# the haystack is a PIPE whose producer status is the thing being managed. The
# `|| true` is load-bearing rather than the swallow-everything idiom the helper
# replaces — and the very next line `die`s when the count is 0, so a tool
# failure still stops the run instead of passing silently.
[ "$(strings "$LIB/libzenohc.so" | grep -c 'ISO-TP: no such interface' || true)" -gt 0 ] ||
    die "$LIB/libzenohc.so has no ISO-TP link in it"
ip link show "$DEV" >/dev/null 2>&1 || die "$DEV is not up; run scripts/test/vcan-setup.sh"

# The ROS peer owns one end of the identifier pair; the nano-ros node owns the
# mirror image. ISO-TP addresses a peer BY the directed pair, so these two
# values are the whole addressing scheme.
ROS_EP="isotp/$DEV#tx_id=0x201;rx_id=0x200"
NODE_EP="isotp/$DEV#tx_id=0x200;rx_id=0x201"

# Derive the session config from the installed default, matched on content
# rather than line numbers so an rmw_zenoh update cannot silently no-op it.
# `$ROS_DISTRO_DIR` is passed IN rather than spelled again inside the heredoc.
# The shell half already resolves `${ROS_DISTRO:-humble}`; a second literal
# `humble` in here is the precise failure this repo's ROS-env gate exists to
# stop -- on a jazzy host it reads a path that does not exist and the harness
# dies deriving a config, with nothing pointing at the distro as the cause.
python3 - "$OUT/session.json5" "$ROS_EP" "$ROS_DISTRO_DIR" <<'PY' || die "could not derive the session config"
import sys
out, endpoint, distro_dir = sys.argv[1], sys.argv[2], sys.argv[3]
src = f"{distro_dir}/share/rmw_zenoh_cpp/config/DEFAULT_RMW_ZENOH_SESSION_CONFIG.json5"
text = open(src).read()
# No router: the default's single connect endpoint is the local rmw_zenohd.
assert '"tcp/localhost:7447"' in text, "default config changed; connect endpoint not found"
text = text.replace('"tcp/localhost:7447"', '// removed: this test has no router')
# ISO-TP as the ONLY listen endpoint. Replacing rather than appending is the
# point: leaving the TCP listener would let the two processes find each other
# without the bus and prove nothing.
assert '"tcp/localhost:0"' in text, "default config changed; listen endpoint not found"
text = text.replace('"tcp/localhost:0"', f'"{endpoint}"')
open(out, "w").write(text)
PY

# Prove the negative rather than asserting it in a comment: no TCP endpoint of
# any kind may survive in the config the peers actually load.
#
# Comment lines are stripped FIRST. json5 configs are heavily documented and the
# stock file's own prose contains `"tcp/10.10.10.10:7447"` as an example -- a
# naive grep matches those and kills a perfectly good run.
sed 's|//.*||' "$OUT/session.json5" > "$OUT/session.stripped"
nros_grep_count _tcp_left '"tcp/' "$OUT/session.stripped"
# shellcheck disable=SC2154  # set by nros_grep_count via printf -v
if [ "$_tcp_left" -ne 0 ]; then
    grep -n '"tcp/' "$OUT/session.stripped" >&2
    die "a TCP endpoint survived in the session config; the run would not prove CAN carried it"
fi
say "ROS peer endpoint: $(grep -o "isotp/[^\"]*" "$OUT/session.json5")  (no router, no TCP)"

# librmw_zenoh_cpp.so names libzenohc.so as a plain DT_NEEDED with no RPATH and
# the vendored library carries no DT_SONAME, so prepending a directory
# substitutes it wholesale. A cargo feature adds no C API: no ROS rebuild.
export LD_LIBRARY_PATH="$LIB:${LD_LIBRARY_PATH:-}"
export RMW_IMPLEMENTATION=rmw_zenoh_cpp
export ZENOH_SESSION_CONFIG_URI="$OUT/session.json5"
# A stray daemon inherits this environment and keeps a session on the bus long
# after the test -- that has corrupted a peer count once already. Humble's
# `ros2 service call` has no `--no-daemon` flag, so stopping it is how you get
# the same guarantee here.
ros2 daemon stop >/dev/null 2>&1 || true

start_candump() { : >"$1"; command -v candump >/dev/null 2>&1 && run_bg candump -ta "$DEV" >"$1" 2>&1; }
report_bus() {
    [ -s "$1" ] || { say "  (candump not installed; no bus capture)"; return; }
    nros_grep_count _ff '\[8\]  1[0-9A-F] ' "$1"
    nros_grep_count _fc '\[3\]  3[0-9A-F] ' "$1"
    # shellcheck disable=SC2154  # set by nros_grep_count via printf -v
    say "  bus: $(wc -l <"$1") frames, $_ff FirstFrames, $_fc FlowControls"
}

# ---------------------------------------------------------------- role: server
# nano-ros SERVES; the ROS 2 CLI calls. The CLI listens and the nano-ros node
# dials out, because the pico ISO-TP link registers on the connect side only.
# `ros2 service call` waits for the service to appear, which is what gives the
# node time to come up and connect.
if [ "$ROLE" = both ] || [ "$ROLE" = all ] || [ "$ROLE" = service-server ]; then
    SERVER=${NANO_ROS_SERVER:?set NANO_ROS_SERVER to the built nano-ros service-server}
    say "role SERVER: nano-ros serves /add_two_ints, ros2 calls it"
    start_candump "$OUT/dump-server.log"
    run_bg timeout 60 ros2 service call /add_two_ints \
        example_interfaces/srv/AddTwoInts "{a: 20, b: 22}" >"$OUT/call.log" 2>&1
    CALL_PID=${PIDS[-1]}
    sleep 3
    NROS_LOCATOR="$NODE_EP" run_bg "$SERVER" >"$OUT/server.log" 2>&1
    wait "$CALL_PID" 2>/dev/null
    kill_all
    nros_grep_count _hits 'sum=42' "$OUT/call.log"
    # shellcheck disable=SC2154  # set by nros_grep_count via printf -v
    if [ "$_hits" -gt 0 ]; then
        say "  PASS: ros2 -> CAN -> nano-ros server -> CAN -> ros2, sum=42"
        report_bus "$OUT/dump-server.log"
    else
        FAILED=1; say "  FAIL"
        echo "--- ros2 service call ---"; tail -20 "$OUT/call.log"
        echo "--- nano-ros server ---";   tail -20 "$OUT/server.log"
    fi
fi

# ---------------------------------------------------------------- role: client
# The mirror image: a ROS 2 node serves and the nano-ros node calls it.
if [ "$ROLE" = both ] || [ "$ROLE" = all ] || [ "$ROLE" = service-client ]; then
    CLIENT=${NANO_ROS_CLIENT:?set NANO_ROS_CLIENT to the built nano-ros service-client}
    say "role CLIENT: ros2 serves /add_two_ints, nano-ros calls it"
    start_candump "$OUT/dump-client.log"
    # The node binary DIRECTLY, not `ros2 run`. The wrapper starts the node in
    # its own session, so neither the pid nor the process group this script can
    # see reaches it, and the node outlives the run holding an ISO-TP session on
    # the bus -- where it then answers a later test's flow control and makes a
    # perfectly good implementation look like it duplicates every frame.
    ROS_SERVER_BIN="$ROS_DISTRO_DIR/lib/demo_nodes_cpp/add_two_ints_server"
    [ -x "$ROS_SERVER_BIN" ] || die "$ROS_SERVER_BIN is not executable"
    run_bg "$ROS_SERVER_BIN" >"$OUT/ros-server.log" 2>&1
    sleep 6
    NROS_LOCATOR="$NODE_EP" run_bg timeout 45 "$CLIENT" >"$OUT/client.log" 2>&1
    CLIENT_PID=${PIDS[-1]}
    wait "$CLIENT_PID" 2>/dev/null
    kill_all
    # Match the example's own result line. An earlier version grepped for `42`
    # -- the number the OTHER role uses -- and passed on a substring of a
    # liveliness keyexpr, which is random hex. This example sends 2 + 3, so 5 is
    # the only correct answer and the label has to be part of the match.
    nros_grep_count _hits 'Result of add_two_ints: 5' "$OUT/client.log"
    # shellcheck disable=SC2154  # set by nros_grep_count via printf -v
    if [ "$_hits" -gt 0 ]; then
        say "  PASS: nano-ros client -> CAN -> ros2 server -> CAN -> nano-ros, 2 + 3 = 5"
        report_bus "$OUT/dump-client.log"
    else
        FAILED=1; say "  FAIL"
        echo "--- nano-ros client ---"; tail -25 "$OUT/client.log"
        echo "--- ros2 server ---";     tail -20 "$OUT/ros-server.log"
    fi
fi

# ------------------------------------------------------- role: action-server
# nano-ros SERVES the action; the ROS 2 CLI drives it. Actions are the harder
# case than services and the reason they are tested separately: one goal is a
# whole conversation -- goal request, accept, a stream of feedback, a result
# request, and a status topic -- so it exercises queries, replies AND pushed
# data across the same ISO-TP face at once.
if [ "$ROLE" = all ] || [ "$ROLE" = action-server ]; then
    ASERVER=${NANO_ROS_ACTION_SERVER:?set NANO_ROS_ACTION_SERVER to the built nano-ros action-server}
    say "role ACTION-SERVER: nano-ros serves /fibonacci, ros2 sends a goal"
    start_candump "$OUT/dump-aserver.log"
    run_bg timeout 90 ros2 action send_goal --feedback /fibonacci \
        example_interfaces/action/Fibonacci "{order: 5}" >"$OUT/asend.log" 2>&1
    ASEND_PID=${PIDS[-1]}
    sleep 3
    NROS_LOCATOR="$NODE_EP" run_bg "$ASERVER" >"$OUT/aserver.log" 2>&1
    wait "$ASEND_PID" 2>/dev/null
    kill_all
    # Match the terminal status on the ROS side AND the server's own completion:
    # a goal that is accepted and then never finishes would still print a goal
    # id, and that must not read as a pass.
    #
    # NOT a formatted sequence string. `ros2 action send_goal` prints the result
    # as a YAML BLOCK LIST ("sequence:" then "- 0", "- 1", ...), not
    # `sequence=[0, 1, ...]`; asserting the latter failed a run that had in fact
    # completed correctly, which is worse than no assertion at all.
    if [ "$(count_in "$OUT/asend.log" 'Goal finished with status: SUCCEEDED')" -gt 0 ] \
       && [ "$(count_in "$OUT/aserver.log" 'Goal succeeded')" -gt 0 ]; then
        say "  PASS: ros2 -> CAN -> nano-ros action server -> CAN -> ros2, [0,1,1,2,3,5]"
        report_bus "$OUT/dump-aserver.log"
    else
        FAILED=1; say "  FAIL"
        echo "--- ros2 action send_goal ---"; tail -25 "$OUT/asend.log"
        echo "--- nano-ros action server ---"; tail -25 "$OUT/aserver.log"
    fi
fi

# ------------------------------------------------------- role: action-client
# The mirror image: a ROS 2 node serves and the nano-ros node drives the goal.
if [ "$ROLE" = all ] || [ "$ROLE" = action-client ]; then
    ACLIENT=${NANO_ROS_ACTION_CLIENT:?set NANO_ROS_ACTION_CLIENT to the built nano-ros action-client}
    say "role ACTION-CLIENT: ros2 serves /fibonacci, nano-ros sends a goal"
    start_candump "$OUT/dump-aclient.log"
    # The node binary directly, not `ros2 run` -- see the SERVER role above.
    AROS_BIN="$ROS_DISTRO_DIR/lib/examples_rclcpp_minimal_action_server/action_server_member_functions"
    [ -x "$AROS_BIN" ] || die "$AROS_BIN is not executable"
    run_bg "$AROS_BIN" >"$OUT/aros-server.log" 2>&1
    sleep 6
    NROS_LOCATOR="$NODE_EP" run_bg timeout 90 "$ACLIENT" >"$OUT/aclient.log" 2>&1
    ACLIENT_PID=${PIDS[-1]}
    wait "$ACLIENT_PID" 2>/dev/null
    kill_all
    # The example sends order = 10 and Zephyr's/ROS's minimal action server
    # emits ELEVEN terms, ending 55 -- not twelve ending 89. Assert the exact
    # sequence rather than a lone number: it is self-documenting, and an
    # off-by-one in either implementation shows up as a failure instead of
    # passing on a substring.
    #
    # Assert the RESULT, never feedback alone: feedback without a result is a
    # goal that started and never finished, which is the failure worth catching.
    if [ "$(count_in "$OUT/aclient.log" 'Result received: \[0, 1, 1, 2, 3, 5, 8, 13, 21, 34, 55\]')" -gt 0 ]; then
        say "  PASS: nano-ros action client -> CAN -> ros2 action server -> CAN -> nano-ros"
        printf '    %s\n' "$(grep -o 'Result received: .*' "$OUT/aclient.log" | tail -1)"
        printf '    feedback samples: %s\n' "$(count_in "$OUT/aclient.log" 'Next number in sequence received')"
        report_bus "$OUT/dump-aclient.log"
    else
        FAILED=1; say "  FAIL"
        echo "--- nano-ros action client ---"; tail -30 "$OUT/aclient.log"
        echo "--- ros2 action server ---"; tail -20 "$OUT/aros-server.log"
    fi
fi

# ------------------------------------------------------- role: action-cancel
# A goal that is CANCELLED, not one that completes. The cancel request is a
# service call on `<action>/_action/cancel_goal`, and the reply has to come back
# across the bus before the client's own timeout -- so this exercises the one
# part of the action protocol the happy path never touches.
#
# It needs a PACED server. ROS 2's cancel client cancels exactly 3.0 s after
# sending the goal, and the stock nano-ros action server computes the whole
# sequence in the tick that sees the goal -- about four milliseconds -- so the
# goal has always succeeded long before the cancel arrives and the cancel path
# is unreachable. Build the example with NROS_FIB_STEP_TICKS=5000 (a
# compile-time knob: the crate is no_std) to emit one term per 5000 ticks.
if [ "$ROLE" = all ] || [ "$ROLE" = action-cancel ]; then
    PSERVER=${NANO_ROS_ACTION_SERVER_PACED:?set NANO_ROS_ACTION_SERVER_PACED to a paced action-server}
    say "role ACTION-CANCEL: nano-ros serves /fibonacci, ros2 cancels the goal"
    start_candump "$OUT/dump-cancel.log"
    CANCEL_BIN="$ROS_DISTRO_DIR/lib/examples_rclcpp_minimal_action_client/action_client_not_composable_with_cancel"
    [ -x "$CANCEL_BIN" ] || die "$CANCEL_BIN is not executable"
    run_bg timeout 60 "$CANCEL_BIN" >"$OUT/cancel-cli.log" 2>&1
    CANCEL_PID=${PIDS[-1]}
    sleep 3
    NROS_LOCATOR="$NODE_EP" run_bg "$PSERVER" >"$OUT/cancel-srv.log" 2>&1
    wait "$CANCEL_PID" 2>/dev/null
    kill_all
    # BOTH ends, deliberately. The client alone would report "Goal was canceled"
    # on a request that timed out client-side; the server's own line is what says
    # the cancel actually crossed the bus and was honoured.
    if [ "$(count_in "$OUT/cancel-cli.log" 'Goal was canceled')" -gt 0 ] \
       && [ "$(count_in "$OUT/cancel-srv.log" 'Goal canceled')" -gt 0 ]; then
        say "  PASS: ros2 cancels -> CAN -> nano-ros honours it, both ends agree"
        report_bus "$OUT/dump-cancel.log"
    else
        FAILED=1; say "  FAIL"
        echo "--- ros2 cancel client ---"; tail -20 "$OUT/cancel-cli.log"
        echo "--- nano-ros action server ---"; tail -20 "$OUT/cancel-srv.log"
    fi
fi

exit $FAILED
