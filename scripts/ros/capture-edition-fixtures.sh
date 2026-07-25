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

image="osrf/ros:${DISTRO}-ros-base"

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

# One container run collects every hash (cheaper than one run per type).
hash_script='source /opt/ros/'"$DISTRO"'/setup.bash
for t in '"${TYPES[*]}"'; do
  h="$(ros2 interface hash "$t" 2>/dev/null || echo "MISSING")"
  echo "$t $h"
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
