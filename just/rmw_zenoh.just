set working-directory := '..'

# rmw_zenoh module — an OPT-IN source build of `rmw_zenoh_cpp`.
#
# NOT how you get a working ROS-side zenoh RMW: install
# `ros-<distro>-rmw-zenoh-cpp` from apt (declared in `nros-sdk-index.toml`,
# available for humble/iron/jazzy). That is what every lane uses, and the
# submodule stays uninitialised until this recipe asks for it.
#
# What this is FOR (RFC-0075): reproducing a SPECIFIC `rmw_zenoh_cpp` ↔
# zenoh-pico pairing on demand — a debugging tool for a suspected wire
# incompatibility. RFC-0075 rejected both making it the default (pinning our own
# RMW makes the tested configuration less like production) and requiring it when
# ROS is present (an hour-long build for no benefit).
#
# Note the historical rationale — "so the versions wire-match" — was refuted by
# issue 0291: zenoh's wire is proto-`0x09`-stable across 1.x, so zpico 1.7.2
# interops with a far newer distro RMW, and that investigation's real finding was
# the keyexpr type-hash. So reach for this when you have a reason, not by default.
#
# The overlay lives at `build/rmw_zenoh_ws/install/`; the harness layers it on
# top of the distro setup when present.

default:
    @just --list --justfile {{source_file()}}

# Build rmw_zenoh_cpp + zenoh_cpp_vendor into an ament overlay.
# Skipped if already built — use `just rmw_zenoh clean && just rmw_zenoh setup`
# to rebuild.
#
# The colcon workspace lives OUTSIDE the repo tree so `zenoh_cpp_vendor`'s
# internal cargo build (pinned Rust 1.75) doesn't walk up into our
# `edition = "2024"` root workspace manifest. The final `install/` dir is
# symlinked back into `build/rmw_zenoh_ws/install/` for discoverability.
[group("setup")]
setup:
    #!/usr/bin/env bash
    set -e
    if [ -f build/rmw_zenoh_ws/install/setup.bash ]; then
        echo "rmw_zenoh overlay already built: build/rmw_zenoh_ws/install/setup.bash"
        echo "To rebuild, run: just rmw_zenoh clean && just rmw_zenoh setup"
        exit 0
    fi
    # Phase 123.A.8 — kept inline: this recipe pulls only the
    # rmw_zenoh dev fixture (one submodule), which the canonical
    # `tools/setup.sh --rmw=zenoh` would NOT include because it
    # lives in `rmw.zenoh.dev_paths`. Adding `--with-dev` would pull
    # zenoh-pico + mbedtls + the full zenoh router too, far more
    # than this recipe needs.
    if [ ! -f third-party/zenoh/rmw_zenoh/rmw_zenoh_cpp/package.xml ]; then
        echo "Initialising rmw_zenoh submodule..."
        git submodule update --init third-party/zenoh/rmw_zenoh
    fi
    if ! command -v colcon &>/dev/null; then
        # phase-327 W3 — remedy derived from [python.colcon]: pip works on any
        # host; the apt package name only exists where the ROS apt repo is
        # configured (issue 0368 F7).
        echo "Error: colcon not found. Install it with:"
        echo "  pip3 install --user colcon-common-extensions"
        echo "  (or your distro package manager on a ROS-apt host)"
        exit 1
    fi
    if [ ! -f /opt/ros/humble/setup.bash ]; then
        echo "Error: /opt/ros/humble not found. ROS 2 Humble is required to build rmw_zenoh_cpp."
        exit 1
    fi
    # Scratch workspace outside the repo so nested cargo builds don't
    # discover our root `Cargo.toml`.
    scratch="${XDG_CACHE_HOME:-$HOME/.cache}/nros/rmw_zenoh_ws"
    mkdir -p "$scratch/src"
    ln -sfn "$(pwd)/third-party/zenoh/rmw_zenoh" "$scratch/src/rmw_zenoh"
    echo "Building rmw_zenoh_cpp overlay in $scratch (this may take several minutes)..."
    bash -c "
      set -e
      source /opt/ros/humble/setup.bash
      cd '$scratch'
      colcon build --merge-install \
          --packages-select zenoh_cpp_vendor rmw_zenoh_cpp
    "
    # Symlink the install dir back into build/ so tests find it at a
    # stable in-repo path (build/rmw_zenoh_ws/install/).
    mkdir -p build/rmw_zenoh_ws
    ln -sfn "$scratch/install" build/rmw_zenoh_ws/install
    echo "✓ rmw_zenoh overlay built: build/rmw_zenoh_ws/install/setup.bash"

# Diagnose rmw_zenoh install status (read-only).
[group("setup")]
doctor:
    #!/usr/bin/env bash
    set +e
    if [ -f build/rmw_zenoh_ws/install/setup.bash ]; then
        lib=build/rmw_zenoh_ws/install/lib/librmw_zenoh_cpp.so
        if [ -f "$lib" ]; then
            echo "  [OK] rmw_zenoh overlay: $lib"
            exit 0
        else
            echo "  [PARTIAL] setup.bash exists but librmw_zenoh_cpp.so missing"
            echo "            Run: just rmw_zenoh clean && just rmw_zenoh setup"
            exit 1
        fi
    else
        echo "  [MISSING] rmw_zenoh overlay (run: just rmw_zenoh setup)"
        exit 1
    fi

# Clean the rmw_zenoh colcon workspace (both the scratch tree and the symlink).
[group("main")]
clean:
    #!/usr/bin/env bash
    set -e
    scratch="${XDG_CACHE_HOME:-$HOME/.cache}/nros/rmw_zenoh_ws"
    rm -rf "$scratch"
    rm -rf build/rmw_zenoh_ws
