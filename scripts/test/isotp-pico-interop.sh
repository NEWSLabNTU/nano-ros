#!/usr/bin/env bash
# RFC-0083 / phase-394 W3 — zenoh-pico <-> zenoh-rs over ISO-TP on vcan0.
#
#   ZENOH_RS_EXAMPLES=<dir> PICO_EXAMPLES=<dir> ./scripts/test/isotp-pico-interop.sh
#
# Needs vcan0 up (scripts/test/vcan-setup.sh) and the can-isotp kernel module.
#
# zenoh-rs LISTENS and zenoh-pico CONNECTS. That asymmetry is deliberate, not a
# limitation of the harness: the pico ISO-TP link registers in `_z_open_link`
# only, so a pico peer dials out and never reaches the accept path, which wants
# a socket and an accept() that no bus has.
#
# Test 2 is the point of the whole phase. zenoh routes queries to unicast faces
# only, so the multicast CAN link of RFC-0080 cannot carry one; this one can,
# and that is what ROS services, actions, parameters and graph introspection
# are built on.
set -o pipefail

RS=${ZENOH_RS_EXAMPLES:-$HOME/repos/zenoh/target/release/examples}
PICO=${PICO_EXAMPLES:?set PICO_EXAMPLES to the built zenoh-pico examples directory}
OUT=$(mktemp -d)
DEV=${CAN_DEV:-vcan0}
RS_EP="isotp/$DEV#tx_id=0x201;rx_id=0x200"
PICO_EP="isotp/$DEV#tx_id=0x200;rx_id=0x201"
PIDS=()
FAILED=0

# `grep -c` prints 0 AND exits 1 when there is no match, so `|| echo 0` would
# emit "0\n0". Swallow the status instead.
count_in() { grep -c "$2" "$1" 2>/dev/null || true; }

# Children are killed by PID. `pkill -f` on a pattern as broad as the example
# names once took the whole run down before its first line of output.
kill_all() {
    for p in "${PIDS[@]}"; do kill "$p" 2>/dev/null || true; done
    sleep 1
    for p in "${PIDS[@]}"; do kill -9 "$p" 2>/dev/null || true; done
    PIDS=()
}
trap 'kill_all; rm -rf "$OUT"' EXIT

# Every process below is KILLED rather than allowed to exit, and block-buffered
# stdout is DISCARDED on SIGTERM. Without stdbuf a run that worked perfectly
# logs nothing at all, which reads exactly like a lost message -- it cost a
# debugging cycle once already.
run_bg() { stdbuf -o0 -e0 "$@" & PIDS+=($!); }

echo "### Test 1: pico z_pub  ->  zenoh-rs z_sub"
run_bg "$RS/z_sub" -m peer -l "$RS_EP" --no-multicast-scouting -k 'demo/**' >"$OUT/sub.log" 2>&1
sleep 3
run_bg "$PICO/z_pub" -m client -e "$PICO_EP" -k demo/example/pico -v hello-from-pico >"$OUT/pub.log" 2>&1
sleep 12
kill_all
N=$(count_in "$OUT/sub.log" hello-from-pico)
echo "  subscriber received: $N"
if [ "$N" -gt 0 ]; then echo "  PASS"; else
    FAILED=1; echo "  FAIL"; echo "--- sub ---"; tail -20 "$OUT/sub.log"; echo "--- pub ---"; tail -20 "$OUT/pub.log"
fi

echo
echo "### Test 2: pico z_get  ->  zenoh-rs z_queryable   (the query)"
run_bg "$RS/z_queryable" -m peer -l "$RS_EP" --no-multicast-scouting \
    -k demo/example/zenoh-rs-queryable -p reply-from-zenoh-rs >"$OUT/qable.log" 2>&1
sleep 3
run_bg "$PICO/z_get" -m client -e "$PICO_EP" -k demo/example/zenoh-rs-queryable >"$OUT/get.log" 2>&1
sleep 15
kill_all
M=$(count_in "$OUT/get.log" reply-from-zenoh-rs)
echo "  replies received by pico: $M"
if [ "$M" -gt 0 ]; then echo "  PASS"; else
    FAILED=1; echo "  FAIL"; echo "--- get ---"; tail -25 "$OUT/get.log"; echo "--- queryable ---"; tail -15 "$OUT/qable.log"
fi

exit $FAILED
