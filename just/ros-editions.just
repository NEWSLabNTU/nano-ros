# Multi-edition ROS test harness (RFC-0058 / phase-309) — INTERNAL test infra.
#
# A host installs one ROS 2 edition; these recipes run codegen + interop against
# OTHER editions inside per-edition docker images (built locally on demand).
# Opt-in — never part of `just ci`. See the `ros-editions-ci` composite (W6).

_DOCKER_DIR := justfile_directory() / "docker" / "ros-editions"
_FIX := justfile_directory() / "packages" / "testing" / "nros-tests" / "bins" / "ros-edition-pose-pub"

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

# Build the nano-ros CycloneDDS PoseStamped publisher fixture against a ROS
# edition's message definitions (W5 residual). geometry_msgs is regenerated from
# the edition's defs (in-container when the image exists, else host ROS), then
# the publisher is built on the host. Consumed by the nano-ros interop lane.
[group("test")]
build-fixture distro="jazzy":
    #!/usr/bin/env bash
    set -euo pipefail
    FIX="{{_FIX}}"
    NROS="$(command -v nros || echo "{{justfile_directory()}}/packages/cli/target/release/nros")"
    [ -x "$NROS" ] || { echo "error: nros CLI not found — run \`just setup-cli\`" >&2; exit 1; }
    if docker image inspect nano-ros-ros:{{distro}} >/dev/null 2>&1; then
        echo "regenerating geometry_msgs against {{distro}} defs (container)"
        docker run --rm \
            -v "$NROS":/usr/local/bin/nros:ro \
            -v "$FIX":/fix \
            nano-ros-ros:{{distro}} \
            bash -lc 'source /opt/ros/{{distro}}/setup.bash && cd /fix && nros generate-rust --force'
    else
        echo "image nano-ros-ros:{{distro}} absent — generating against host ROS"
        ( cd "$FIX" && "$NROS" generate-rust --force )
    fi
    ( cd "$FIX" && cargo build )
    echo "build-fixture {{distro}}: $FIX/target/debug/ros-edition-pose-pub"

# Build the six native example nodes (talker/listener/service-{server,client}/
# action-{server,client}) with the CycloneDDS RMW into per-edition build dirs
# (target-ros-edition-<distro>), for the phase-310 rich E2E matrix. The examples
# build against their committed (host) generated msgs — the interop types
# (Int32, AddTwoInts, Fibonacci) are edition-stable. Separate target dirs keep
# the default/host builds untouched.
[group("test")]
build-e2e-fixtures distro="jazzy":
    #!/usr/bin/env bash
    set -euo pipefail
    for ex in talker listener service-server service-client action-server action-client; do
        DIR="{{justfile_directory()}}/examples/native/rust/$ex"
        echo "== $ex (cyclone) → target-ros-edition-{{distro}} =="
        ( cd "$DIR" && cargo build --no-default-features --features rmw-cyclonedds \
            --target-dir "target-ros-edition-{{distro}}" )
    done
    echo "build-e2e-fixtures {{distro}}: 6 binaries under examples/native/rust/*/target-ros-edition-{{distro}}/debug/"

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
    # Build the nano-ros publisher fixture (bridge lane) + the six example nodes
    # (rich E2E matrix) against this edition.
    just ros_editions build-fixture {{distro}}
    just ros_editions build-e2e-fixtures {{distro}}
    echo "== ros-editions harness lanes ({{distro}}) =="
    export NROS_ROS_EDITION={{distro}}
    cargo test -p nros-tests \
        --test ros_editions_smoke \
        --test ros_editions_codegen \
        --test ros_editions_bridge \
        --test ros_editions_nano_interop \
        --test ros_editions_e2e_pubsub \
        --test ros_editions_e2e_service \
        --test ros_editions_e2e_action \
        -- --nocapture
    echo "ros_editions ci {{distro}}: PASS"
