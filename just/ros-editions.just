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
        ros2 pkg prefix rmw_zenoh_cpp >/dev/null 2>&1 && echo "[ok] rmw_zenoh_cpp" || echo "[--] rmw_zenoh_cpp absent (install ros-<distro>-rmw-zenoh-cpp)"
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
    # shellcheck source=scripts/build/cargo.sh
    source "{{justfile_directory()}}/scripts/build/cargo.sh"
    NROS="$(nros_cli_bin)" || { echo "error: nros CLI not found — run \`just setup-cli\`" >&2; exit 1; }
    if docker image inspect nano-ros-ros:{{distro}} >/dev/null 2>&1; then
        echo "regenerating geometry_msgs against {{distro}} defs (container)"
        docker run --rm \
            -v "$NROS":/usr/local/bin/nros:ro \
            -v "$FIX":/fix \
            nano-ros-ros:{{distro}} \
            bash -lc 'source /opt/ros/{{distro}}/setup.bash && cd /fix && nros dev codegen rust --force'
    else
        echo "image nano-ros-ros:{{distro}} absent — generating against host ROS"
        ( cd "$FIX" && "$NROS" dev codegen rust --force )
    fi
    # issue 0488 residue 1 — into `$NROS_BUILD_ROOT`, not the fixture's own leaf.
    # Same reasoning as the e2e loop below: `generated/` is rewritten per edition,
    # so this cannot share a lane group, but it has no business writing
    # `packages/testing/**/target/` either. Coordinate = the edition.
    # shellcheck source=scripts/build/build-root.sh
    source "{{justfile_directory()}}/scripts/build/build-root.sh"
    FIX_TARGET_DIR="$(nros_build_dir "$NROS_KIND_ROS_EDITIONS" "pose-pub-{{distro}}")"
    mkdir -p "$FIX_TARGET_DIR"
    # shellcheck source=scripts/build/cargo.sh
    source "{{justfile_directory()}}/scripts/build/cargo.sh"
    nros_seed_leaf_lock "$FIX"
    # Issue 1305 — the leaf names no board or RMW; its `system.toml` image does.
    # `ws leaf-system --write-settings` writes the image's selection package and
    # settings file WITHOUT re-running codegen (a `nros configure` here would
    # regenerate `generated/` from the HOST's definitions, undoing the edition's
    # above). Cargo then builds through that file, from the directory above the
    # leaf, as `nros build` does (scripts/build/leaf-settings.sh records why).
    SETTINGS="$(NROS_REPO_DIR="{{justfile_directory()}}" "$NROS" dev image leaf-system --write-settings "$FIX" \
        | sed -n 's/^NROS_LEAF_SETTINGS=//p')"
    [ -f "$SETTINGS" ] || { echo "error: no settings file for $FIX" >&2; exit 1; }
    ( cd "$(dirname "$FIX")" && cargo build --manifest-path "$FIX/Cargo.toml" \
        --config "$SETTINGS" --target-dir "$FIX_TARGET_DIR" )
    # profile-literal-ok: unprofiled: the docker edition fixtures build with a plain `cargo build`
    echo "build-fixture {{distro}}: $FIX_TARGET_DIR/debug/ros-edition-pose-pub"

# Build the six native example nodes (talker/listener/service-{server,client}/
# action-{server,client}) for one (ROS edition, RMW) into per-(edition, rmw)
# build dirs, for the phase-310/311 rich E2E matrix. `rmw` ∈ {cyclone, zenoh,
# xrce}.
#
# Each example is a one-node workspace (phase-531 / phase-535 W6), and a
# workspace generates its messages for ONE edition, refusing an image of
# another (issue 2409). So each edition build is a COPY of the example's
# workspace whose `[system] ros_edition` is this edition, under the edition's
# build root, built through `nros build`. The image ids are the examples'
# (`linux`, then `_<rmw>` unless zenoh), and each binary is copied to
# `debug/<example>`, the name `ros_env::example_bin_rmw` resolves. Each copy
# has its own cargo target dir: a target dir serves one workspace root (issue
# 0616).
[group("test")]
build-e2e-fixtures distro="jazzy" rmw="cyclone" dry_run="":
    #!/usr/bin/env bash
    set -euo pipefail
    case "{{rmw}}" in
        cyclone) RMW_ID=cyclonedds ;;
        zenoh)   RMW_ID=zenoh ;;
        xrce)    RMW_ID=xrce ;;
        *) echo "unknown rmw '{{rmw}}' (want cyclone|zenoh|xrce)" >&2; exit 1 ;;
    esac
    # The distro name IS the edition name (jazzy/iron/humble). The edition puts
    # the real RIHS01 type-hash tail in the zenoh keyexpr so a jazzy node
    # matches a stock jazzy `rmw_zenoh_cpp` peer (#0291).
    ED="{{distro}}"
    WS_IMG="linux"
    if [ "$RMW_ID" != zenoh ]; then WS_IMG="${WS_IMG}_${RMW_ID}"; fi
    # shellcheck source=scripts/build/cargo.sh
    source "{{justfile_directory()}}/scripts/build/cargo.sh"
    NROS="$(nros_cli_bin)" || { echo "error: nros CLI not found — run \`just setup-cli\`" >&2; exit 1; }
    # issue 0488 residue 2 — ONE build root per (edition, rmw), under
    # `$NROS_BUILD_ROOT`, instead of six per-leaf `target-ros-edition-*/` dirs.
    #
    # These builds deliberately do NOT join a fixture group: the messages are
    # regenerated per edition, so the same source dir means different code per
    # edition. Sharing a cargo dir with the normal `linux` lane would thrash both.
    # `<root>/<kind>/<coordinate>` is RFC-0070 R2's shape, and (edition, rmw) is
    # the coordinate these actually vary on.
    # shellcheck source=scripts/build/build-root.sh
    source "{{justfile_directory()}}/scripts/build/build-root.sh"
    ED_TARGET_DIR="$(nros_build_dir "$NROS_KIND_ROS_EDITIONS" "{{distro}}-{{rmw}}")"
    # profile-literal-ok: unprofiled: `nros build` of a host image with no `profile`, so debug/ IS the derived dir
    mkdir -p "$ED_TARGET_DIR/debug"
    for ex in talker listener service-server service-client action-server action-client; do
        SRC="{{justfile_directory()}}/examples/linux/rust/$ex"
        BRINGUP="$(cd "$SRC/src" && ls -d *_bringup)"
        [ "$(echo "$BRINGUP" | wc -w)" = 1 ] \
            || { echo "error: $SRC is not a one-node workspace (bringups: $BRINGUP)" >&2; exit 1; }
        WS="$ED_TARGET_DIR/$ex-ws"
        echo "== $ex (workspace copy, ros_edition $ED, image $BRINGUP:$WS_IMG) → $ED_TARGET_DIR/debug/$ex =="
        if [ -n "{{dry_run}}" ]; then continue; fi
        rm -rf "$WS/src"
        mkdir -p "$WS"
        cp -r "$SRC/src" "$WS/src"
        cp "$SRC/.colcon_workspace" "$WS/"
        sed -i "s/^\[system\]\$/[system]\nros_edition = \"$ED\"/" "$WS/src/$BRINGUP/system.toml"
        grep -q "^ros_edition = \"$ED\"\$" "$WS/src/$BRINGUP/system.toml" \
            || { echo "error: could not set ros_edition = $ED in the $ex copy" >&2; exit 1; }
        "$NROS" build "$BRINGUP:$WS_IMG" --workspace "$WS" -- --target-dir "$WS/target"
        # profile-literal-ok: unprofiled: as above
        cp -f "$WS/target/debug/${WS_IMG}_entry" "$ED_TARGET_DIR/debug/$ex"
    done
    # profile-literal-ok: unprofiled: as above
    echo "build-e2e-fixtures {{distro}} {{rmw}}: 6 binaries under $ED_TARGET_DIR/debug/ (image $WS_IMG)"

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
    # shellcheck source=scripts/build/cargo.sh
    source scripts/build/cargo.sh
    if ! nros_cli_bin >/dev/null 2>&1; then
        echo "error: codegen lane needs the nros CLI — run \`just setup-cli\` first" >&2
        exit 1
    fi
    # Build the nano-ros publisher fixture (bridge lane) + the six example nodes
    # (rich E2E matrix) against this edition — cyclone + xrce RMW builds.
    just ros_editions build-fixture {{distro}}
    just ros_editions build-e2e-fixtures {{distro}} cyclone
    just ros_editions build-e2e-fixtures {{distro}} xrce
    just ros_editions build-e2e-fixtures {{distro}} zenoh
    export NROS_ROS_EDITION={{distro}}
    echo "== ros-editions harness lanes ({{distro}}) — codegen/bridge =="
    NROS_RMW=cyclone cargo test -p nros-tests \
        --test ros_editions_smoke \
        --test ros_editions_codegen \
        --test ros_editions_bridge \
        --test ros_editions_nano_interop \
        -- --nocapture
    # Issue 0327 — the pub/sub/service/action × direction interop cells for all
    # three RMWs live in ONE parametrized binary (`ros_editions_e2e`); the RMW
    # prefix on each case name selects the lane + its threading.
    echo "== ros-editions harness lanes ({{distro}}) — cyclone interop (rmw_cyclonedds_cpp) =="
    NROS_RMW=cyclone cargo test -p nros-tests \
        --test ros_editions_e2e cyclone \
        -- --nocapture
    echo "== ros-editions harness lanes ({{distro}}) — XRCE (Agent + fastrtps) =="
    NROS_RMW=xrce cargo test -p nros-tests \
        --test ros_editions_e2e xrce \
        -- --nocapture --test-threads=1
    echo "== ros-editions harness lanes ({{distro}}) — Zenoh (rmw_zenohd + rmw_zenoh_cpp) =="
    # Serial: one host rmw_zenohd on tcp/7447 at a time. Skips loudly on editions
    # without rmw_zenoh_cpp (iron/humble); jazzy runs 6/6 (measured 2026-10-15,
    # issue 1913 — every action endpoint on its own hash). Interop key is the
    # RIHS01 keyexpr, NOT a zenoh bump (#0291).
    NROS_RMW=zenoh cargo test -p nros-tests \
        --test ros_editions_e2e zenoh \
        -- --nocapture --test-threads=1
    echo "ros_editions ci {{distro}}: PASS (cyclone + xrce + zenoh)"
