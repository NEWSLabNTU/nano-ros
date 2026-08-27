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

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=scripts/lib/grep-q.sh
. "$script_dir/../lib/grep-q.sh"

die() { echo "isotp-pico-interop: $*" >&2; exit 2; }

# zenoh-rs is a separate upstream checkout the developer builds themselves. It
# defines no nros-* profile, so `release/` here is ITS profile dir, not one of
# ours to propagate. `ZENOH_RS_EXAMPLES` is the real interface; the default is
# only a convenience for the common clone location.
# profile-literal-ok: vendored
RS=${ZENOH_RS_EXAMPLES:-$HOME/repos/zenoh/target/release/examples}
PICO=${PICO_EXAMPLES:?set PICO_EXAMPLES to the built zenoh-pico examples directory}
OUT=$(mktemp -d)
DEV=${CAN_DEV:-vcan0}
RS_EP="isotp/$DEV#tx_id=0x201;rx_id=0x200"
PICO_EP="isotp/$DEV#tx_id=0x200;rx_id=0x201"
PIDS=()
FAILED=0

# Counting goes through the shared helper, not `grep -c … || true`. That idiom
# handles no-match (prints 0, exits 1) and silently swallows a tool FAILURE
# (prints nothing, exits >=2) as the same thing — so a log that was never
# created reads as "zero messages received", i.e. a delivery failure this test
# would then report as its own finding. See scripts/lib/grep-q.sh.

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

# Preconditions FAIL, loudly and by name. Without these the harness still
# "runs": the examples never exec, both counts come back 0, and the report is
# two FAILs that look exactly like an ISO-TP regression. A test that cannot run
# must say so rather than produce a verdict (CLAUDE.md, "tests must fail on
# unmet preconditions") — the sibling isotp-ros-interop.sh already does this and
# this script did not.
[ -d "$RS" ]   || die "zenoh-rs examples not found at $RS (set ZENOH_RS_EXAMPLES)"
[ -x "$RS/z_sub" ] && [ -x "$RS/z_queryable" ] ||
    die "$RS has no z_sub/z_queryable — build zenoh-rs examples first"
[ -x "$PICO/z_pub" ] && [ -x "$PICO/z_get" ] ||
    die "$PICO has no z_pub/z_get — build the zenoh-pico examples first"
ip link show "$DEV" >/dev/null 2>&1 || die "$DEV is not up; run scripts/test/vcan-setup.sh"
# The link is ISO-TP, not raw CAN: without the can-isotp module every endpoint
# fails to open and the symptom is, again, silence on both tests.
if [ -r /proc/net/protocols ]; then
    nros_grep_count _isotp '^CAN_ISOTP' /proc/net/protocols
    # shellcheck disable=SC2154  # set by nros_grep_count via printf -v
    [ "$_isotp" -gt 0 ] || die "the can-isotp kernel module is not loaded (modprobe can-isotp)"
fi

echo "### Test 1: pico z_pub  ->  zenoh-rs z_sub"
run_bg "$RS/z_sub" -m peer -l "$RS_EP" --no-multicast-scouting -k 'demo/**' >"$OUT/sub.log" 2>&1
sleep 3
run_bg "$PICO/z_pub" -m client -e "$PICO_EP" -k demo/example/pico -v hello-from-pico >"$OUT/pub.log" 2>&1
sleep 12
kill_all
nros_grep_count N hello-from-pico "$OUT/sub.log"
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
nros_grep_count M reply-from-zenoh-rs "$OUT/get.log"
echo "  replies received by pico: $M"
if [ "$M" -gt 0 ]; then echo "  PASS"; else
    FAILED=1; echo "  FAIL"; echo "--- get ---"; tail -25 "$OUT/get.log"; echo "--- queryable ---"; tail -15 "$OUT/qable.log"
fi

exit $FAILED
