#!/usr/bin/env bash
# phase-303 / issue #0267 — minimal live domain_bridge interop harness.
#
# Replaces the heavy Autoware demo with SIMPLE ROS 2 test nodes: a publisher on
# domain 2 → `domain_bridge` (2→1) → a downstream echo on domain 1, over a
# nested-`Time` message (the #0267 corruption shape). Asserts the downstream
# decodes the published values intact.
#
#   ┌──────────────┐  domain 2   ┌──────────────┐  domain 1   ┌───────────────┐
#   │  publisher   │────────────▶│ domain_bridge│────────────▶│  echo (check) │
#   └──────────────┘             └──────────────┘             └───────────────┘
#
# Two publisher modes:
#   --publisher stock      : a stock `ros2 topic pub` (BASELINE — proves the
#                            harness + that pure-Jazzy is clean; all XCDR1).
#   --publisher external   : NO internal publisher — expects an EXTERNAL node
#                            (e.g. a `ros-jazzy`-built nano-ros talker, cyclone
#                            RMW) publishing on domain 2. This is the #0267 fix
#                            verification: nano-ros's appendable descriptor (W1c)
#                            must let a Jazzy peer decode it across the bridge.
#
# Usage:
#   scripts/ros/domain-bridge-repro.sh [--publisher stock|external]
#                                      [--distro jazzy]
#                                      [--type geometry_msgs/msg/PoseStamped]
#                                      [--topic pose] [--timeout 30]
#
# For --publisher external the container runs with `--network host` so a nano-ros
# node on the HOST (or another container on host net) reaches the same DDS
# domains. Point nano-ros at `ROS_DOMAIN_ID=2`, RMW = cyclonedds, publishing the
# same `--type` on `--topic`.
#
# Requires: docker. Pulls ros:<distro>-ros-base + installs ros-<distro>-domain-bridge.

set -euo pipefail

PUBLISHER=stock
DISTRO=jazzy
TYPE="geometry_msgs/msg/PoseStamped"
TOPIC=pose
TIMEOUT=30
# Known payload the stock publisher sends (PoseStamped default: nested Time + a
# signed float). Overridable with --values; the pass-check substrings default per
# --type (or set explicitly with --expect a,b,c).
VALUES=""
EXPECT_ARG=""

while [ $# -gt 0 ]; do
  case "$1" in
    --publisher) PUBLISHER="$2"; shift 2 ;;
    --distro)    DISTRO="$2"; shift 2 ;;
    --type)      TYPE="$2"; shift 2 ;;
    --topic)     TOPIC="$2"; shift 2 ;;
    --timeout)   TIMEOUT="$2"; shift 2 ;;
    --values)    VALUES="$2"; shift 2 ;;
    --expect)    EXPECT_ARG="$2"; shift 2 ;;
    -h|--help)   sed -n '2,40p' "$0"; exit 0 ;;
    *) echo "unknown arg: $1" >&2; exit 2 ;;
  esac
done

# Per-type defaults for VALUES (stock publisher) + EXPECT (pass-check substrings).
declare -a EXPECT
case "$TYPE" in
  *PoseStamped)
    [ -z "$VALUES" ] && VALUES='{header: {stamp: {sec: 7, nanosec: 9}, frame_id: map}, pose: {position: {x: 1.5, y: 2.5, z: -3.5}, orientation: {w: 1.0}}}'
    EXPECT=("sec: 7" "nanosec: 9" "x: 1.5" "z: -3.5" "w: 1.0") ;;
  *Header)
    [ -z "$VALUES" ] && VALUES='{stamp: {sec: 7, nanosec: 9}, frame_id: map}'
    EXPECT=("sec: 7" "nanosec: 9" "frame_id: map") ;;
  *)
    [ -z "$VALUES" ] && VALUES='{}'
    EXPECT=() ;;
esac
# Explicit --expect a,b,c overrides the per-type default.
if [ -n "$EXPECT_ARG" ]; then
  IFS=',' read -r -a EXPECT <<< "$EXPECT_ARG"
fi

command -v docker >/dev/null || { echo "error: docker required" >&2; exit 2; }

IMAGE="${NROS_ROS_IMAGE:-ros:${DISTRO}-ros-base}"
NET=()
[ "$PUBLISHER" = external ] && NET=(--network host)

# The in-container orchestration.
read -r -d '' SCRIPT <<INNER || true
set -e
export DEBIAN_FRONTEND=noninteractive
apt-get update -qq >/dev/null 2>&1
apt-get install -y -qq ros-${DISTRO}-domain-bridge >/dev/null 2>&1
source /opt/ros/${DISTRO}/setup.bash

cat > /tmp/bridge.yaml <<YAML
name: nros_0267_bridge
topics:
  ${TOPIC}:
    type: ${TYPE}
    from_domain: 2
    to_domain: 1
YAML

# Downstream echo on domain 1 (the checker).
( ROS_DOMAIN_ID=1 timeout $((TIMEOUT)) ros2 topic echo --once /${TOPIC} ${TYPE} > /tmp/out.txt 2>&1 ) &
ECHO=\$!
sleep 3
# The bridge (2 -> 1).
( timeout $((TIMEOUT)) ros2 run domain_bridge domain_bridge /tmp/bridge.yaml > /tmp/bridge.log 2>&1 ) &
sleep 4

if [ "${PUBLISHER}" = stock ]; then
  ( ROS_DOMAIN_ID=2 timeout $((TIMEOUT-8)) ros2 topic pub -r 5 /${TOPIC} ${TYPE} "${VALUES}" > /tmp/pub.log 2>&1 ) &
else
  echo "[harness] --publisher external: waiting for a node on ROS_DOMAIN_ID=2 topic /${TOPIC} (${TYPE})..." >&2
fi

wait \$ECHO 2>/dev/null || true
echo "===DOWNSTREAM==="
cat /tmp/out.txt
INNER

OUT="$(docker run --rm "${NET[@]}" "$IMAGE" bash -c "$SCRIPT" 2>&1 | sed -n '/===DOWNSTREAM===/,$p' | tail -n +2)"

echo "── downstream (domain 1) received ─────────────────────────────"
echo "$OUT"
echo "───────────────────────────────────────────────────────────────"

if [ -z "$OUT" ] || echo "$OUT" | grep -qiE "no message|timed out"; then
  echo "FAIL: downstream received nothing (publisher not present / bridge down)." >&2
  [ "$PUBLISHER" = external ] && echo "      (external mode: is the nano-ros publisher running on domain 2?)" >&2
  exit 1
fi

fail=0
for e in "${EXPECT[@]}"; do
  if ! echo "$OUT" | grep -qF "$e"; then
    echo "FAIL: downstream is MISSING expected \"$e\" — value corrupted across the bridge." >&2
    fail=1
  fi
done
if [ "$fail" -ne 0 ]; then
  echo "⇒ #0267 CORRUPTION REPRODUCED (values did not survive the domain_bridge republish)." >&2
  exit 1
fi

echo "PASS: all ${#EXPECT[@]} values survived the domain_bridge republish (no #0267 corruption)."
