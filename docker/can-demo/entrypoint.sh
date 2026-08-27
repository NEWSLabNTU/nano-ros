#!/usr/bin/env bash
# ROS 2 over CAN, end to end, self-verifying (RFC-0082 / phase-387).
#
#   (default)    run the multicast demo (topics) and assert it worked
#   --negative   run the deliberately-broken variant and assert it FAILS
#   --unicast    run the ISO-TP demo: a ROS 2 SERVICE CALL over CAN, plus the
#                same call over the multicast link, which must fail
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
UNICAST=0
case "${1:-}" in
    --negative) NEGATIVE=1 ;;
    --unicast)  UNICAST=1 ;;
esac

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

# ---------------------------------------------------------------------------
# --unicast: a ROS 2 service call over CAN, and the same call over the
# multicast link, which cannot carry it.
# ---------------------------------------------------------------------------
# This is the whole argument of RFC-0083 in one run. zenoh routes queries to
# UNICAST faces only. The multicast CAN link of RFC-0080 therefore carries
# topics and nothing built on a query -- no services, no actions, no
# parameters, no graph introspection. ISO 15765-2 gives CAN a real unicast
# face and all of them come back.
#
# Both halves run on purpose. A demo that shows only the working case leaves
# the reader to take the broken case on trust.
if [ "$UNICAST" -eq 1 ]; then
    # `can-isotp` is a separate kernel module from `vcan`, and the container
    # cannot load it: it takes no privileges beyond NET_ADMIN by design.
    if ! grep -qw can_isotp /proc/modules 2>/dev/null && [ ! -d /sys/module/can_isotp ]; then
        fail "the host kernel has no can-isotp module loaded.
     sudo modprobe can-isotp
     (Debian/Ubuntu may need linux-modules-extra-\$(uname -r))"
    fi

    # ISO-TP is point to point: one peer listens on the identifier pair and the
    # other dials it. Whichever role a peer has, the OTHER endpoint slot is
    # emptied -- leaving a TCP one would let the two find each other without the
    # bus and the demo would prove nothing.
    gen_unicast_config() {
        python3 - "$1" "$2" "$3" <<'UCPY'
import sys
src = "/opt/ros/humble/share/rmw_zenoh_cpp/config/DEFAULT_RMW_ZENOH_SESSION_CONFIG.json5"
out, endpoint, role = sys.argv[1], sys.argv[2], sys.argv[3]
text = open(src).read()
assert '"tcp/localhost:7447"' in text, "default config changed; connect endpoint not found"
assert '"tcp/localhost:0"' in text, "default config changed; listen endpoint not found"
if role == "listen":
    text = text.replace('"tcp/localhost:7447"', '// removed: no router in this demo')
    text = text.replace('"tcp/localhost:0"', '"%s"' % endpoint)
else:
    text = text.replace('"tcp/localhost:7447"', '"%s"' % endpoint)
    text = text.replace('"tcp/localhost:0"', '// removed: this peer dials out')
open(out, "w").write(text)
UCPY
    }

    # The negative half, on the MULTICAST link: there every peer listens,
    # because a bus has no connection setup.
    gen_multicast_config() {
        python3 - "$1" "$2" <<'MCPY'
import sys
src = "/opt/ros/humble/share/rmw_zenoh_cpp/config/DEFAULT_RMW_ZENOH_SESSION_CONFIG.json5"
out, ident = sys.argv[1], sys.argv[2]
text = open(src).read()
text = text.replace('"tcp/localhost:7447"', '// removed: no router in this demo')
text = text.replace('"tcp/localhost:0"',
    '"can/vcan0#bitrate=500000;dbitrate=2000000;id=%s;match=0;mask=0;so_rcvbuf=8388608"' % ident)
open(out, "w").write(text)
MCPY
    }

    # The node BINARY, not `ros2 run`: the wrapper starts the node in its own
    # session, so the pid here would not reach it and it would outlive the step
    # still holding the identifier pair.
    SRV_BIN=/opt/ros/humble/lib/demo_nodes_cpp/add_two_ints_server
    [ -x "$SRV_BIN" ] || fail "$SRV_BIN is missing from the image"

    rule
    say "3u. the SAME service call over the MULTICAST link -- expected to fail"
    gen_multicast_config "$LOGS/mc-server.json5" 0x300
    gen_multicast_config "$LOGS/mc-client.json5" 0x301
    ZENOH_SESSION_CONFIG_URI="$LOGS/mc-server.json5" "$SRV_BIN" > "$LOGS/mc-server.log" 2>&1 &
    MC_SRV=$!
    sleep 8
    ZENOH_SESSION_CONFIG_URI="$LOGS/mc-client.json5" timeout 30 ros2 service call /add_two_ints \
        example_interfaces/srv/AddTwoInts "{a: 20, b: 22}" > "$LOGS/mc-call.log" 2>&1
    MC_RC=$?
    kill "$MC_SRV" 2>/dev/null; wait "$MC_SRV" 2>/dev/null
    MC_OK=$(grep -c 'sum=42' "$LOGS/mc-call.log" 2>/dev/null || true)
    printf '    multicast service call: rc=%s, replies=%s\n' "$MC_RC" "${MC_OK:-0}"

    rule
    say "4u. the same call over the ISO-TP UNICAST link"
    gen_unicast_config "$LOGS/uc-server.json5" "isotp/$DEV#tx_id=0x201;rx_id=0x200" listen
    gen_unicast_config "$LOGS/uc-client.json5" "isotp/$DEV#tx_id=0x200;rx_id=0x201" connect
    grep -o 'isotp/[^"]*' "$LOGS/uc-server.json5" | sed 's/^/    server: /'
    grep -o 'isotp/[^"]*' "$LOGS/uc-client.json5" | sed 's/^/    client: /'

    candump -ta "$DEV" > "$LOGS/uc-candump.log" 2>&1 &
    UC_DUMP=$!
    ZENOH_SESSION_CONFIG_URI="$LOGS/uc-server.json5" "$SRV_BIN" > "$LOGS/uc-server.log" 2>&1 &
    UC_SRV=$!
    sleep 8
    ZENOH_SESSION_CONFIG_URI="$LOGS/uc-client.json5" timeout 60 ros2 service call /add_two_ints \
        example_interfaces/srv/AddTwoInts "{a: 20, b: 22}" > "$LOGS/uc-call.log" 2>&1
    UC_RC=$?
    kill "$UC_SRV" "$UC_DUMP" 2>/dev/null; wait "$UC_SRV" 2>/dev/null
    UC_OK=$(grep -c 'sum=42' "$LOGS/uc-call.log" 2>/dev/null || true)

    rule
    say "5u. results"
    sed 's/^/    /' "$LOGS/uc-call.log"
    echo "    ISO 15765-2 on the wire:"
    printf '      first frames  (1x): %s\n' "$(grep -cE '\[8\]  1[0-9A-F] ' "$LOGS/uc-candump.log" || true)"
    printf '      flow controls (3x): %s\n' "$(grep -cE '\[3\]  3[0-9A-F] ' "$LOGS/uc-candump.log" || true)"
    echo "    frames by CAN identifier:"
    awk '{print $3}' "$LOGS/uc-candump.log" 2>/dev/null | sort | uniq -c | sed 's/^/      /'

    rule
    say "6u. checking expectations"
    [ "${UC_OK:-0}" -gt 0 ] || fail \
        "the service call over ISO-TP returned no reply. Logs: $LOGS/uc-call.log,
     $LOGS/uc-server.log"
    [ "${MC_OK:-0}" -eq 0 ] || fail \
        "the service call SUCCEEDED over the multicast link. That contradicts
     RFC-0083's premise, so either the config did not take effect or something
     other than CAN carried it -- both would invalidate the unicast result."
    say "PASS: a ROS 2 service call crossed CAN over ISO-TP and returned sum=42,"
    say "      while the same call over the multicast CAN link returned nothing."
    say "      Queries route to unicast faces only; that is a property of zenoh's"
    say "      multicast transport, not a limitation of CAN."
    exit 0
fi

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
say "    THIS link. A zenoh multicast face routes pushed data only, and that is"
say "    a property of zenoh's multicast transport, not a limitation of CAN."
say "    They DO work over the ISO-TP unicast link: run --unicast to see a"
say "    service call cross this same bus and come back."
say "  - Nothing about timing. vcan has no bit rate and no arbitration."
say "  - Bus load is the publisher's whole output, not the subscribers' interest."
rule
