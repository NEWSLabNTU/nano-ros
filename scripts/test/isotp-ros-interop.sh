#!/usr/bin/env bash
# RFC-0083 / phase-394 W6 — ROS 2 SERVICE CALLS over CAN, in both roles.
#
#   ZENOHC_ISOTP_LIB=<dir with the isotp libzenohc.so> \
#   NANO_ROS_SERVER=<built nano-ros service-server> \
#   NANO_ROS_CLIENT=<built nano-ros service-client> \
#   ./scripts/test/isotp-ros-interop.sh [--role server|client|both]
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

say() { echo "[isotp-ros] $*"; }
die() { echo "[isotp-ros] error: $*" >&2; exit 1; }
count_in() { grep -c "$2" "$1" 2>/dev/null || true; }

kill_all() {
    for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null || true; done
    sleep 1
    for p in "${PIDS[@]}"; do kill -9 "$p" 2>/dev/null || true; done
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
run_bg() { stdbuf -o0 -e0 "$@" & PIDS+=($!); }

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
python3 - "$OUT/session.json5" "$ROS_EP" <<'PY' || die "could not derive the session config"
import sys
src = "/opt/ros/humble/share/rmw_zenoh_cpp/config/DEFAULT_RMW_ZENOH_SESSION_CONFIG.json5"
out, endpoint = sys.argv[1], sys.argv[2]
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
if [ "$(grep -c '"tcp/' "$OUT/session.stripped" || true)" -ne 0 ]; then
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
    say "  bus: $(wc -l <"$1") frames, $(grep -cE '\[8\]  1[0-9A-F] ' "$1" || true) FirstFrames, $(grep -cE '\[3\]  3[0-9A-F] ' "$1" || true) FlowControls"
}

# ---------------------------------------------------------------- role: server
# nano-ros SERVES; the ROS 2 CLI calls. The CLI listens and the nano-ros node
# dials out, because the pico ISO-TP link registers on the connect side only.
# `ros2 service call` waits for the service to appear, which is what gives the
# node time to come up and connect.
if [ "$ROLE" = both ] || [ "$ROLE" = server ]; then
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
    if [ "$(count_in "$OUT/call.log" 'sum=42')" -gt 0 ]; then
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
if [ "$ROLE" = both ] || [ "$ROLE" = client ]; then
    CLIENT=${NANO_ROS_CLIENT:?set NANO_ROS_CLIENT to the built nano-ros service-client}
    say "role CLIENT: ros2 serves /add_two_ints, nano-ros calls it"
    start_candump "$OUT/dump-client.log"
    run_bg ros2 run demo_nodes_cpp add_two_ints_server >"$OUT/ros-server.log" 2>&1
    sleep 6
    NROS_LOCATOR="$NODE_EP" run_bg timeout 45 "$CLIENT" >"$OUT/client.log" 2>&1
    CLIENT_PID=${PIDS[-1]}
    wait "$CLIENT_PID" 2>/dev/null
    kill_all
    # The example prints the sum it got back; accept either the bare value or a
    # labelled form so a cosmetic change to the example does not fail the gate.
    if [ "$(count_in "$OUT/client.log" '42')" -gt 0 ]; then
        say "  PASS: nano-ros client -> CAN -> ros2 server -> CAN -> nano-ros, got 42"
        report_bus "$OUT/dump-client.log"
    else
        FAILED=1; say "  FAIL"
        echo "--- nano-ros client ---"; tail -25 "$OUT/client.log"
        echo "--- ros2 server ---";     tail -20 "$OUT/ros-server.log"
    fi
fi

exit $FAILED
