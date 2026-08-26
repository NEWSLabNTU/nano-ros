#!/usr/bin/env bash
# ROS 2 over CAN, end to end, self-verifying (RFC-0082 / phase-387).
#
#   (default)    run the demo and assert it worked
#   --negative   run the deliberately-broken variant and assert it FAILS
#
# Exits nonzero if any assertion fails. A demo that prints logs and leaves the
# reader to judge proves nothing to someone who has never seen the system.
# Deliberately not `set -u`: ROS's setup.bash references unset variables and
# would abort the script on the very first thing it does.
set -o pipefail

DEV=vcan0
TALKER_ID=0x100
LISTENER_ID=0x101
PICO_ID=0x200
RUN_SECONDS=${RUN_SECONDS:-20}
LOGS=/tmp/can-demo
NEGATIVE=0
[ "${1:-}" = "--negative" ] && NEGATIVE=1

say()  { echo "[can-demo] $*"; }
fail() { echo "[can-demo] FAIL: $*" >&2; exit 1; }
rule() { echo "-----------------------------------------------------------"; }

mkdir -p "$LOGS" && rm -f "$LOGS"/*

# shellcheck disable=SC1091
source /opt/ros/humble/setup.bash
# Ahead of the vendored library, which is the whole substitution mechanism.
export LD_LIBRARY_PATH=/opt/can-demo/lib:${LD_LIBRARY_PATH:-}
export RMW_IMPLEMENTATION=rmw_zenoh_cpp

rule
say "1. a virtual CAN bus in this container's own network namespace"
ip link add dev "$DEV" type vcan 2>/dev/null || fail \
    "could not create $DEV. The container needs --cap-add=NET_ADMIN, and the
     HOST kernel must have the vcan module available (sudo modprobe vcan).
     The container cannot load it itself, by design: it takes no privileges."
ip link set up "$DEV" || fail "could not bring up $DEV"
ip -br link show "$DEV" | sed 's/^/    /'

rule
say "2. rmw_zenoh_cpp is using our libzenohc.so, not the vendored one"
# Located through ament, not a literal prefix (issues 0653/0654): the shell here
# cannot reach `nros_zenohd_bin`, but `ros2 pkg prefix` is the same question
# asked path-independently, and it is the idiom scripts/can/build-zenohc-can.sh
# already prints as the operator's instruction.
ZENOHD="$(ros2 pkg prefix rmw_zenoh_cpp)/lib/rmw_zenoh_cpp/rmw_zenohd"
RESOLVED="$(ldd "$ZENOHD" | awk '/libzenohc/{print $3}')"
echo "    $RESOLVED"
case "$RESOLVED" in
    /opt/can-demo/lib/*) : ;;
    *) fail "rmw_zenohd resolved libzenohc.so to '$RESOLVED', not our build" ;;
esac

rule
say "3. session configs: CAN is the ONLY transport"
# Derived from the installed default so the image cannot drift from its base,
# and matched on content rather than line numbers.
gen_config() {
    python3 - "$1" "$2" "$3" <<'PY'
import sys
src = "/opt/ros/humble/share/rmw_zenoh_cpp/config/DEFAULT_RMW_ZENOH_SESSION_CONFIG.json5"
out, endpoint = sys.argv[1], sys.argv[2]
band = sys.argv[3]
text = open(src).read()
# No router: remove the only connect endpoint.
assert '"tcp/localhost:7447"' in text, "default config changed; connect endpoint not found"
text = text.replace('"tcp/localhost:7447"', '// removed: this demo has no router')
# CAN as the only listen endpoint. so_rcvbuf is not optional here: a container's
# vcan has no bit rate, so a burst arrives as fast as memory allows and the
# kernel drops the overflow before the link sees it.
assert '"tcp/localhost:0"' in text, "default config changed; listen endpoint not found"
text = text.replace('"tcp/localhost:0"',
    f'"can/vcan0#bitrate=500000;dbitrate=2000000;id={endpoint};{band};so_rcvbuf=8388608"')
open(out, "w").write(text)
PY
}

if [ "$NEGATIVE" -eq 1 ]; then
    say "    NEGATIVE MODE: putting the two ROS peers in disjoint identifier bands,"
    say "    so the CAN link's own filter separates them. The listener must hear nothing."
    # The listener moves band, so every later reference to it must move too --
    # including the readiness poll, which otherwise waits for an identifier that
    # will never appear and reports a timeout instead of the real result.
    LISTENER_ID=0x201
    gen_config "$LOGS/talker.json5"   "$TALKER_ID"   "match=0x100;mask=0x700"
    gen_config "$LOGS/listener.json5" "$LISTENER_ID" "match=0x200;mask=0x700"
else
    gen_config "$LOGS/talker.json5"   "$TALKER_ID"   "match=0;mask=0"
    gen_config "$LOGS/listener.json5" "$LISTENER_ID" "match=0;mask=0"
fi
grep -A 2 'endpoints: \[' "$LOGS/talker.json5" | grep 'can/' | sed 's/^ */    talker:   /'
grep -A 2 'endpoints: \[' "$LOGS/listener.json5" | grep 'can/' | sed 's/^ */    listener: /'

rule
say "4. starting peers"
candump -ta "$DEV" > "$LOGS/candump.log" 2>&1 &
CANDUMP=$!

/opt/can-demo/pico/z_sub -m peer -k '**' \
    -l "can/$DEV#bitrate=500000;dbitrate=2000000;id=$PICO_ID;match=0;mask=0" \
    > "$LOGS/pico.log" 2>&1 &
PICO=$!

ZENOH_SESSION_CONFIG_URI="$LOGS/listener.json5" \
    ros2 run demo_nodes_cpp listener > "$LOGS/listener.log" 2>&1 &
LISTENER=$!

# Poll for readiness rather than sleeping. Multicast peers learn about each
# other on the next periodic Join, 2.5 s apart, so a fixed short sleep is
# indistinguishable from a hang and a fixed long one is still a guess. Frames
# appearing from both identifiers is the observable signal that they are up.
say "    waiting for the listener and the pico peer to appear on the bus"
READY=0
for _ in $(seq 1 60); do
    if grep -q " ${LISTENER_ID#0x} " "$LOGS/candump.log" 2>/dev/null \
       && grep -q " ${PICO_ID#0x} " "$LOGS/candump.log" 2>/dev/null; then
        READY=1; break
    fi
    sleep 1
done
[ "$READY" -eq 1 ] || fail "the listener and/or the pico peer never transmitted on $DEV"
say "    both are on the bus"

rule
say "5. publishing for ${RUN_SECONDS}s over CAN, with no router and no TCP"
ZENOH_SESSION_CONFIG_URI="$LOGS/talker.json5" \
    timeout "$RUN_SECONDS" ros2 run demo_nodes_cpp talker > "$LOGS/talker.log" 2>&1
sleep 2
kill "$LISTENER" "$PICO" "$CANDUMP" 2>/dev/null
wait "$LISTENER" "$PICO" 2>/dev/null

# `grep -c` PRINTS 0 and EXITS 1 when there are no matches, so `|| echo 0`
# appends a second line and the count becomes "0\n0", which every later
# integer test then chokes on. `|| true` keeps grep's own 0 and swallows only
# the exit status.
count_in() { local n; n="$(grep -c "$1" "$2" 2>/dev/null || true)"; echo "${n:-0}"; }
PUBLISHED=$(count_in "Publishing" "$LOGS/talker.log")
HEARD=$(count_in "I heard" "$LOGS/listener.log")
PICO_GOT=$(count_in "chatter" "$LOGS/pico.log")

rule
say "6. results"
printf '    ROS talker published        : %s\n' "$PUBLISHED"
printf '    ROS listener heard          : %s\n' "$HEARD"
printf '    zenoh-pico peer heard       : %s  (chatter frames)\n' "$PICO_GOT"
echo "    frames on the bus, by CAN identifier:"
awk '{print $3}' "$LOGS/candump.log" 2>/dev/null | sort | uniq -c | sed 's/^/      /'

rule
if [ "$NEGATIVE" -eq 1 ]; then
    say "7. checking the NEGATIVE expectations"
    [ "$PUBLISHED" -gt 0 ] || fail "the talker published nothing; the run was broken, not the bands"
    [ "$HEARD" -eq 0 ] || fail \
        "the listener heard $HEARD messages across disjoint identifier bands.
     Either the band filter is not working, or something other than CAN
     carried the traffic -- both would invalidate the positive run."
    say "PASS: bands isolated the peers. The talker published $PUBLISHED and the"
    say "      listener heard 0, while both kept transmitting -- so CAN is"
    say "      provably the carrier in the positive run, not some other path."
    exit 0
fi

say "7. checking expectations"
[ "$PUBLISHED" -gt 0 ] || fail "the talker published nothing"
[ "$HEARD" -ge $((PUBLISHED - 1)) ] || fail \
    "the listener heard $HEARD of $PUBLISHED messages"
[ "$PICO_GOT" -gt 0 ] || fail \
    "the zenoh-pico peer received nothing. If zenoh-pico was built with a
     BATCH_MULTICAST_SIZE other than 63 it never associates, and the only
     symptom is one INFO line on its side: $LOGS/pico.log"
for id in "${TALKER_ID#0x}" "${LISTENER_ID#0x}" "${PICO_ID#0x}"; do
    grep -q " $id " "$LOGS/candump.log" || fail "no frames from identifier $id"
done

say "PASS"
say "  A ROS 2 topic crossed a CAN bus to another ROS 2 node and to a"
say "  zenoh-pico peer, with no router and no TCP endpoint anywhere."
rule
say "What this does NOT show:"
say "  - Services, actions, parameters and graph introspection do not work over"
say "    this transport. A zenoh multicast face routes pushed data only."
say "  - Nothing about timing. vcan has no bit rate and no arbitration."
say "  - Bus load is the publisher's whole output, not the subscribers' interest."
rule
