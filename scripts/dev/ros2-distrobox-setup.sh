#!/usr/bin/env bash
# ROS 2 Humble inside an Ubuntu 22.04 distrobox, for a nano-ros host that
# cannot have ROS natively (Arch, Fedora 40+, NixOS …).
#
# Full walkthrough, including how to create the box:
#     docs/development/ros2-on-non-ubuntu.md
#
# Run INSIDE the box, from the checkout:
#     distrobox enter ros2 -- bash scripts/dev/ros2-distrobox-setup.sh
#
# `sudo` here is the CONTAINER's root, not the host's — the box is disposable
# (`distrobox rm ros2`) and nothing outside it is touched. $HOME is shared with
# the host by design, which is what the NROS_HOME / CARGO_TARGET_DIR notes at
# the bottom are about.
set -euo pipefail

if [ ! -f /run/.containerenv ] && [ ! -f /.dockerenv ]; then
    echo "refusing to run: this is meant to run INSIDE the distrobox, not on the host." >&2
    echo "  distrobox enter ros2 -- bash $0" >&2
    exit 1
fi

. /etc/os-release
if [ "${VERSION_ID:-}" != "22.04" ]; then
    echo "refusing to run: ROS 2 Humble targets Ubuntu 22.04, this box is ${PRETTY_NAME:-unknown}." >&2
    echo "  (jazzy wants 24.04 — recreate the box with the matching image.)" >&2
    exit 1
fi

echo "=== [1/4] base tooling + the book's host prerequisites"
sudo apt-get update -qq
sudo apt-get install -y --no-install-recommends \
    ca-certificates curl gnupg lsb-release software-properties-common \
    git build-essential pkg-config cmake ninja-build \
    python3 python3-pip python3-venv python3-dev

echo "=== [2/4] ROS 2 apt repository"
sudo install -d -m 0755 /etc/apt/keyrings
curl -fsSL https://raw.githubusercontent.com/ros/rosdistro/master/ros.key \
    | sudo gpg --dearmor -o /usr/share/keyrings/ros-archive-keyring.gpg
echo "deb [arch=$(dpkg --print-architecture) signed-by=/usr/share/keyrings/ros-archive-keyring.gpg] \
http://packages.ros.org/ros2/ubuntu $(. /etc/os-release && echo "$UBUNTU_CODENAME") main" \
    | sudo tee /etc/apt/sources.list.d/ros2.list >/dev/null
sudo apt-get update -qq

echo "=== [3/4] ROS 2 Humble + what the nano-ros lanes actually use"
# desktop = ros2 CLI, rclcpp/rclpy, the interface packages, rviz. The middlewares
# and domain_bridge are what the interop cells drive; colcon + rosidl-adapter are
# what codegen and the workspace lanes need.
sudo apt-get install -y --no-install-recommends \
    ros-humble-desktop \
    ros-humble-rmw-cyclonedds-cpp \
    ros-humble-rmw-fastrtps-cpp \
    ros-humble-domain-bridge \
    ros-humble-example-interfaces \
    ros-humble-rosidl-adapter \
    python3-colcon-common-extensions \
    python3-rosdep python3-vcstool
# NOTE: rmw_zenoh_cpp has NO humble apt package (it starts at iron). The zenoh
# interop cells build it as a pinned source overlay into build/rmw_zenoh_ws/ —
# `just rmw_zenoh setup` in the repo. Cyclone and FastRTPS interop work without it.

echo "=== [4/4] verify"
# ROS's setup.bash reads AMENT_TRACE_SETUP_FILES and friends without defaults,
# so it dies instantly under `set -u`. Every ROS-sourcing script needs this.
set +u
# shellcheck disable=SC1091
source /opt/ros/humble/setup.bash
set -u
ros2 --help >/dev/null && echo "[ok] ros2 CLI"
for p in rmw_cyclonedds_cpp rmw_fastrtps_cpp domain_bridge example_interfaces geometry_msgs std_msgs; do
    ros2 pkg prefix "$p" >/dev/null 2>&1 && echo "[ok] $p" || echo "[--] $p MISSING"
done
ros2 pkg prefix rmw_zenoh_cpp >/dev/null 2>&1 \
    && echo "[ok] rmw_zenoh_cpp" \
    || echo "[--] rmw_zenoh_cpp absent — expected on humble; build the overlay for zenoh interop"

cat <<'EOF'

=== done ===

$HOME is SHARED with the host, and the host's binaries do NOT run here (glibc
is only backward compatible). `scripts/dev/ros2-box-env.sh` sets the box-local
NROS_HOME / CARGO_TARGET_DIR / CARGO_INSTALL_ROOT that keeps the two apart —
source it rather than exporting them by hand. Then, inside the box:

  cd <your checkout>
  . scripts/dev/ros2-box-env.sh    # box-local store / target dir / PATH
  cargo build --release --manifest-path packages/cli/Cargo.toml --bin nros
  nros_box_publish                 # put the box CLI where consumers look
  nros setup native --rmw zenoh
EOF
