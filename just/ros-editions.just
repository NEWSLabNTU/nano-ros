# Multi-edition ROS test harness (RFC-0058 / phase-309) — INTERNAL test infra.
#
# A host installs one ROS 2 edition; these recipes run codegen + interop against
# OTHER editions inside per-edition docker images (built locally on demand).
# Opt-in — never part of `just ci`. See the `ros-editions-ci` composite (W6).

_DOCKER_DIR := justfile_directory() / "docker" / "ros-editions"

# Build the per-edition ROS image locally (cached). First build is a few minutes
# (apt only — no source compile). Tag: nano-ros-ros:<distro>.
[group("test")]
image distro="jazzy":
    docker build --build-arg DISTRO={{distro}} \
        -t nano-ros-ros:{{distro}} \
        -f "{{_DOCKER_DIR}}/Dockerfile" "{{_DOCKER_DIR}}"

# Verify a built image has the capabilities the harness needs: ros2 CLI, the
# three middlewares (best-effort zenoh), domain_bridge, and the interface pkgs.
[group("test")]
image-check distro="jazzy":
    #!/usr/bin/env bash
    set -euo pipefail
    IMG="nano-ros-ros:{{distro}}"
    docker image inspect "$IMG" >/dev/null 2>&1 || { echo "image $IMG not built — run: just ros_editions image {{distro}}" >&2; exit 1; }
    docker run --rm "$IMG" bash -lc '
        set -e
        source /opt/ros/{{distro}}/setup.bash
        ros2 --help >/dev/null            && echo "[ok] ros2 CLI"
        ros2 pkg prefix rmw_cyclonedds_cpp >/dev/null && echo "[ok] rmw_cyclonedds_cpp"
        ros2 pkg prefix rmw_fastrtps_cpp   >/dev/null && echo "[ok] rmw_fastrtps_cpp"
        ros2 pkg prefix domain_bridge      >/dev/null && echo "[ok] domain_bridge"
        ros2 pkg prefix example_interfaces >/dev/null && echo "[ok] example_interfaces"
        ros2 pkg prefix geometry_msgs      >/dev/null && echo "[ok] geometry_msgs"
        ros2 pkg prefix rmw_zenoh_cpp >/dev/null 2>&1 && echo "[ok] rmw_zenoh_cpp" || echo "[--] rmw_zenoh_cpp absent (apt-less distro; source overlay needed)"
    '
    echo "image-check {{distro}}: PASS"

# Composite: build the edition image + run the gated harness lanes (peer smoke,
# codegen axis, domain_bridge interop). OPT-IN — NOT part of `just ci` (docker +
# a built image + slow first build). Adding another edition = re-run with its
# distro arg once the image builds (W2-W5 are distro-parametric).
#
# Prereqs: docker; the in-tree `nros` CLI for the codegen lane (`just setup-cli`).
[group("test")]
ci distro="jazzy":
    #!/usr/bin/env bash
    set -euo pipefail
    just ros_editions image {{distro}}
    just ros_editions image-check {{distro}}
    if [ ! -x packages/cli/target/release/nros ] && ! command -v nros >/dev/null 2>&1; then
        echo "error: codegen lane needs the nros CLI — run \`just setup-cli\` first" >&2
        exit 1
    fi
    echo "== ros-editions harness lanes ({{distro}}) =="
    cargo test -p nros-tests \
        --test ros_editions_smoke \
        --test ros_editions_codegen \
        --test ros_editions_bridge \
        -- --nocapture
    echo "ros_editions ci {{distro}}: PASS"
