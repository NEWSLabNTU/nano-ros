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
# (prefix with DBX_CONTAINER_MANAGER=docker if the host has podman too, else
# distrobox looks for the box under the wrong manager — see ros2-box-env.sh)
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

# `libmbedtls-dev`: the zpico TLS build discovers mbedTLS through .pc files
# nano-ros GENERATES, so discovery cannot fail — without the dev package the
# build dies later inside a vendored TU on a missing `mbedtls/entropy.h`.
# `clang`/`libclang-dev`: bindgen needs libclang AND its resource headers —
# without them z3-sys fails at `/usr/include/stdio.h: 'stddef.h' file not
# found`, which looks like a broken libc rather than a missing compiler.
# `libz3-dev`: `nros-launch-resolve` deps `z3-sys`, whose bindgen step needs
# `z3.h`. Without it the resolver cannot be built INSIDE the box, and a
# host-built one is unusable here (it links the host's libpython) — so
# `just build-test-fixtures` dies mid-sync with a dynamic-loader error.
# `python3-tomli`: Ubuntu 22.04 ships Python 3.10, which predates `tomllib`
# (3.11+). `just check cargo-profile-mirror` reads Cargo.toml with tomllib and
# falls back to tomli — on a bare box NEITHER exists and tier 1 dies there with
# a bare `ModuleNotFoundError`, long after the ROS parts it came here for.
#
# This list is deliberately the MINIMUM to build the nros CLI, and nothing more.
# Everything else the recipes need is declared in `[system.*]` in
# nros-sdk-index.toml and installed by `nros setup --system` — see the closing
# instructions. Do not grow this list to cover a missing recipe tool: that is the
# hand-written-list drift issue 0368 removed, and a package named in both places
# will diverge from the index the moment one side is edited.
echo "=== [1/4] base tooling + the book's host prerequisites"
sudo apt-get update -qq
sudo apt-get install -y --no-install-recommends \
    ca-certificates curl gnupg lsb-release software-properties-common \
    git build-essential pkg-config cmake ninja-build \
    python3 python3-pip python3-venv python3-dev \
    python3-tomli libz3-dev clang libclang-dev libmbedtls-dev

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
    ros-humble-rmw-zenoh-cpp \
    ros-humble-domain-bridge \
    ros-humble-example-interfaces \
    ros-humble-rosidl-adapter \
    python3-colcon-common-extensions \
    python3-rosdep python3-vcstool
# rmw_zenoh_cpp: this note used to say there is NO humble apt package. There is
# now — `ros-humble-rmw-zenoh-cpp`, candidate 0.1.8-1jammy as of 2026-08-18 —
# so it is installed above rather than built.
#
# There is no longer an alternative to build: the pinned source overlay and its
# submodule were removed (RFC-0075, amended 2026-08-19) after measuring that
# nothing ever used them. The rationale they carried — "so rmw_zenoh_cpp matches
# our zenoh-pico pin" — was refuted by issue 0291: zenoh's wire is proto-0x09
# stable across 1.x and zpico 1.7.2 interops with jazzy's 1.11.2, so the version
# match was never what mattered. Anyone hitting a wire-level mismatch should read
# 0291 before assuming versions are the cause.
#
# Without EITHER, every zenoh e2e lane reports `[SKIPPED:capability]` — a skip
# that reads as green. `just doctor` says which of the two you have.

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
  nros setup --system              # system packages — SEE BELOW, do not skip
  nros setup native --rmw zenoh
  just doctor                      # confirm; fix anything it reports

`nros setup --system` is the step this box goes wrong without, and it fails
QUIETLY. Step [1/4] above installs only what the CLI needs to COMPILE; the
recipes need a further set declared in `[system.*]` of nros-sdk-index.toml, and
most of the justfile probes those with `command -v` and DEGRADES instead of
failing. So a box missing them still builds, still passes, and merely runs
wrong — e.g. without `parallel`, `check-examples` prints "GNU parallel not
found — falling back to serial check" and walks 99 example leaves one at a
time, which on a 4-core box reads as a hang rather than as a missing package.

`just doctor` names every one of them with its remedy in a single line. Run it
before concluding anything about a slow or red box.
EOF
