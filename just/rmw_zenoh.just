set working-directory := '..'

# rmw_zenoh module — build the rmw_zenoh_cpp ROS 2 overlay from the
# pinned submodule so `ros2` CLI commands can reach nros nodes via
# zenoh. The overlay lives at `build/rmw_zenoh_ws/install/` and the
# test harness (nros-tests) sources its `setup.bash` when present.

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
setup:
    #!/usr/bin/env bash
    set -e
    if [ -f build/rmw_zenoh_ws/install/setup.bash ]; then
        echo "rmw_zenoh overlay already built: build/rmw_zenoh_ws/install/setup.bash"
        echo "To rebuild, run: just rmw_zenoh clean && just rmw_zenoh setup"
        exit 0
    fi
    if [ ! -f third-party/zenoh/rmw_zenoh/rmw_zenoh_cpp/package.xml ]; then
        echo "Initialising rmw_zenoh submodule..."
        git submodule update --init third-party/zenoh/rmw_zenoh
    fi
    if ! command -v colcon &>/dev/null; then
        echo "Error: colcon not found. Install it with:"
        echo "  sudo apt install python3-colcon-common-extensions"
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
clean:
    #!/usr/bin/env bash
    set -e
    scratch="${XDG_CACHE_HOME:-$HOME/.cache}/nros/rmw_zenoh_ws"
    rm -rf "$scratch"
    rm -rf build/rmw_zenoh_ws
