#!/usr/bin/env bash
# phase-304 W4 (RFC-0056) — capture per-ROS-edition Tier-A reference fixtures.
#
# The dev/CI host installs only Humble, but the RIHS01 type hash + the
# rosidl_adapter IDL that vary per ROS distro are CAPTURABLE ONCE from a
# throwaway `osrf/ros:<distro>` container and committed as fixtures. The
# in-repo tests then assert the nano-ros codegen reproduces them with NO ROS
# runtime — the container is a capture TOOL, not a test dependency.
#
# What it captures (per type in TYPES below):
#   - RIHS01 type hash: `ros2 interface hash <type>` (Iron+; Humble has none).
#     Feeds phase-304 W1 (real RIHS01 computation) as the golden values.
#   - rosidl_adapter IDL: the `.idl` ROS emits for the type. Feeds the
#     nros-msg-to-idl per-edition parity (phase-303 W1 finding: extensibility
#     annotation, if any, is a per-distro fact).
#
# Usage:  scripts/ros/capture-edition-fixtures.sh <distro>
#   <distro> = iron | jazzy | rolling   (humble has no `ros2 interface hash`)
#
# Output:  packages/testing/nros-tests/fixtures/ros-editions/<distro>/
#            hashes.txt        # "<type> <RIHS01_...>" per line
#            <pkg>__<Name>.idl # rosidl_adapter IDL per type
#
# Requires: docker. Pulls osrf/ros:<distro>-ros-base (~1 GB) on first run.

set -euo pipefail

DISTRO="${1:?usage: capture-edition-fixtures.sh <iron|jazzy|rolling>}"
case "$DISTRO" in
  iron | jazzy | rolling) ;;
  humble)
    echo "error: humble predates 'ros2 interface hash' — no RIHS01 to capture." >&2
    exit 2 ;;
  *)
    echo "error: unknown distro '$DISTRO' (iron | jazzy | rolling)" >&2
    exit 2 ;;
esac

if ! command -v docker >/dev/null 2>&1; then
  echo "error: docker not found — this capture needs a container." >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
out_dir="$repo_root/packages/testing/nros-tests/fixtures/ros-editions/$DISTRO"
mkdir -p "$out_dir"

# The `-ros-base` variants live in the docker-official `ros` repo (NOT `osrf/ros`,
# which only publishes `-desktop`). Override with NROS_ROS_IMAGE if needed.
image="${NROS_ROS_IMAGE:-ros:${DISTRO}-ros-base}"

# A small, representative type set: a flat primitive msg, a nested-struct msg
# (Header + fields), and a service — enough to exercise the RIHS01 canonical
# form's nested-type closure + the service split.
TYPES=(
  "std_msgs/msg/Int32"
  "std_msgs/msg/Header"
  "geometry_msgs/msg/Twist"
  "sensor_msgs/msg/Imu"
  "example_interfaces/srv/AddTwoInts"
)

echo "capturing $DISTRO fixtures from $image → $out_dir"

# The RIHS01 hash is NOT a CLI subcommand (`ros2 interface hash` does not
# exist on Iron/Jazzy either). rosidl generates a per-type description JSON at
# `share/<pkg>/<kind>/<Name>.json` carrying `type_hashes[].hash_string` — read
# that. Also copy the whole `.json` (the canonical type description) so the
# nano-ros engine's `to_hashable_json` can be diffed structurally.
hash_script='source /opt/ros/'"$DISTRO"'/setup.bash
for t in '"${TYPES[*]}"'; do
  pkg="${t%%/*}"; rest="${t#*/}"; kind="${rest%%/*}"; name="${rest##*/}"
  share="$(ros2 pkg prefix "$pkg" 2>/dev/null)/share/$pkg/$kind/$name.json"
  if [ -f "$share" ]; then
    h="$(python3 -c "import json,sys; d=json.load(open(sys.argv[1])); print(next(x[\"hash_string\"] for x in d[\"type_hashes\"] if x[\"type_name\"]==\"$t\"))" "$share" 2>/dev/null || echo MISSING)"
    echo "$t $h"
  else
    echo "$t MISSING(no-json)"
  fi
done'

docker run --rm "$image" bash -c "$hash_script" > "$out_dir/hashes.txt"

echo "wrote $out_dir/hashes.txt:"
sed 's/^/  /' "$out_dir/hashes.txt"

# rosidl_adapter IDL per msg type (services split into two structs; skip here).
for t in "${TYPES[@]}"; do
  case "$t" in */msg/*) ;; *) continue ;; esac
  pkg="${t%%/*}"
  name="${t##*/}"
  idl_script='source /opt/ros/'"$DISTRO"'/setup.bash
python3 -c "
from rosidl_adapter.parser import parse_message_file
from rosidl_adapter.resource import expand_template
import os, ament_index_python as aip
share = aip.get_package_share_directory(\"'"$pkg"'\")
msg = os.path.join(share, \"msg\", \"'"$name"'.msg\")
print(open(msg).read())
" 2>/dev/null || true'
  docker run --rm "$image" bash -c "$idl_script" > "$out_dir/${pkg}__${name}.msg" || true
done

echo "done. Commit $out_dir as the $DISTRO reference fixtures."
echo "NOTE: phase-304 W1 asserts the nano-ros RIHS01 computation reproduces hashes.txt."
