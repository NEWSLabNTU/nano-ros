#!/bin/bash
# Build Micro-XRCE-DDS Agent from source
#
# Builds Micro-XRCE-DDS-Agent from the submodule at third-party/xrce/agent.
# The Agent is needed for XRCE-DDS integration tests (just xrce test).
#
# Usage:
#   ./scripts/xrce-agent/build.sh [--clean]
#
# Output:
#   build/xrce-agent/MicroXRCEAgent
#
# Prerequisites:
#   - CMake >= 3.5
#   - C++14 compiler (gcc >= 5, clang >= 3.4)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
AGENT_SRC="$REPO_ROOT/third-party/xrce/agent"
# phase-334 W2.b step 2 — RFC-0070 R3: one derivation, not a literal.
# shellcheck source=scripts/build/build-root.sh
source "$REPO_ROOT/scripts/build/build-root.sh"
BUILD_DIR="$(nros_build_dir "$NROS_KIND_XRCE_AGENT")"

# Parse arguments
if [ "$1" = "--clean" ]; then
    echo "Cleaning XRCE Agent build..."
    rm -rf "$BUILD_DIR"
    echo "Done."
    exit 0
fi

# Issue 0741 — when a ROS install is PRESENT (the only case where DDS interop
# matters), the agent must be built against THAT install's Fast-DDS: the
# bundled prebuilt carries a Jazzy-era Fast-DDS (2.14.x/Fast-CDR 2.x), and a
# Humble bus (2.6.x/Fast-CDR 1.x) talking to it is exactly the skew behind
# the 28-byte-reply-into-15-byte-history refusal. The agent TAG is not free:
# its system branch expects a specific Fast-CDR MAJOR — derived here from the
# installed library itself (libfastcdr.so.1 → v2.4.2; .so.2 → v2.4.3), never
# from a distro-name table that would rot.
#
# Detection follows the RFC-0075 router doctrine: only what the user NAMED —
# a sourced environment (AMENT_PREFIX_PATH). No /opt/ros glob, no PATH walk.
nros_ros_fastdds_prefix() {
    local IFS=':'
    for p in ${AMENT_PREFIX_PATH:-}; do
        if [ -n "$p" ] && ls "$p"/lib/libfastcdr.so.* >/dev/null 2>&1                        && ls "$p"/lib/libfastrtps.so.* >/dev/null 2>&1; then
            printf '%s\n' "$p"
            return 0
        fi
    done
    return 1
}

if ros_prefix="$(nros_ros_fastdds_prefix)"; then
    if ls "$ros_prefix"/lib/libfastcdr.so.2* >/dev/null 2>&1; then
        agent_ref="v2.4.3"
    else
        agent_ref="v2.4.2"
    fi
    PAIRED_DIR="$BUILD_DIR/ros-paired"
    stamp="$PAIRED_DIR/.stamp"
    want="$agent_ref $ros_prefix"
    if [ -x "$BUILD_DIR/MicroXRCEAgent" ] && [ -f "$stamp" ]         && [ "$(cat "$stamp" 2>/dev/null)" = "$want" ]; then
        echo "ROS-paired Micro-XRCE-DDS Agent up to date ($agent_ref against $ros_prefix)"
        exit 0
    fi
    echo "Building ROS-paired Micro-XRCE-DDS Agent $agent_ref against $ros_prefix ..."
    mkdir -p "$PAIRED_DIR"
    src="$PAIRED_DIR/src"
    if [ ! -f "$src/CMakeLists.txt" ]; then
        # The in-tree submodule is PINNED (v2.4.3) and pins move forward only,
        # so a v2.4.2 build cannot come from it — shallow-clone the tag.
        git clone --depth 1 --branch "$agent_ref"             https://github.com/eProsima/Micro-XRCE-DDS-Agent "$src"         || { echo "clone failed (offline?) — falling back to the bundled agent" >&2; src=""; }
    elif ! git -C "$src" describe --tags 2>/dev/null | grep -q "$agent_ref"; then
        rm -rf "$src"
        git clone --depth 1 --branch "$agent_ref"             https://github.com/eProsima/Micro-XRCE-DDS-Agent "$src"         || { echo "clone failed (offline?) — falling back to the bundled agent" >&2; src=""; }
    fi
    if [ -n "$src" ] && [ -f "$src/CMakeLists.txt" ]; then
        cmake -S "$src" -B "$PAIRED_DIR/build" -DCMAKE_BUILD_TYPE=Release             -DUAGENT_BUILD_EXECUTABLE=ON             -DUAGENT_USE_SYSTEM_FASTDDS=ON -DUAGENT_USE_SYSTEM_FASTCDR=ON             -DUAGENT_P2P_PROFILE=OFF -DUAGENT_LOGGER_PROFILE=OFF             -DUAGENT_SOCKETCAN_PROFILE=OFF >/dev/null
        cmake --build "$PAIRED_DIR/build" --parallel "$(nproc 2>/dev/null || echo 4)"
        # Wrapper (not a copy): the binary links the ROS install's libs; keep
        # them reachable even when the caller forgot to source the env.
        tmp="$BUILD_DIR/MicroXRCEAgent.$$"
        printf '#!/bin/sh\nLD_LIBRARY_PATH="%s/lib:$LD_LIBRARY_PATH" exec "%s" "$@"\n' \
            "$ros_prefix" "$PAIRED_DIR/build/MicroXRCEAgent" > "$tmp"
        chmod 0755 "$tmp"
        mv -f "$tmp" "$BUILD_DIR/MicroXRCEAgent"
        printf '%s' "$want" > "$stamp"
        echo "Published ROS-paired agent: $BUILD_DIR/MicroXRCEAgent ($agent_ref, zero Fast-DDS skew)"
        exit 0
    fi
fi

# Prefer the prebuilt MicroXRCEAgent from the nros SDK store (provisioned by
# `nros setup … --rmw xrce`) — no source build, no submodule, no cmake/g++
# needed. Publish it at build/xrce-agent/MicroXRCEAgent where tests + recipes
# look. Source build below is the fallback for trees without nros provisioning.
NROS_STORE="${NROS_HOME:-$HOME/.nros}/sdk"
store_agent="$(ls -d "$NROS_STORE"/xrce-agent/*/bin/MicroXRCEAgent 2>/dev/null | tail -1 || true)"
if [ -n "$store_agent" ] && [ -x "$store_agent" ]; then
    echo "Using prebuilt Micro-XRCE-DDS Agent from the nros store: $store_agent"
    # The store binary is a relocatable launcher that resolves its bundled
    # `../lib/MicroXRCEAgent.real` relative to itself — so it must run from its
    # own dir. Publish a forwarding wrapper (not a copy) at the expected path.
    mkdir -p "$BUILD_DIR"
    tmp="$BUILD_DIR/MicroXRCEAgent.$$"
    printf '#!/bin/sh\nexec "%s" "$@"\n' "$store_agent" > "$tmp"
    chmod 0755 "$tmp"
    mv -f "$tmp" "$BUILD_DIR/MicroXRCEAgent"
    "$BUILD_DIR/MicroXRCEAgent" --version 2>/dev/null || true
    exit 0
fi

# Check prerequisites
if ! command -v cmake &>/dev/null; then
    echo "Error: cmake not found"
    echo "Install: sudo apt install cmake"
    exit 1
fi

if ! command -v g++ &>/dev/null && ! command -v clang++ &>/dev/null; then
    echo "Error: C++ compiler not found"
    echo "Install: sudo apt install g++"
    exit 1
fi

# No store agent — fall back to a source build, but only if the submodule is
# already checked out. Provisioning is `nros setup`'s job; don't silently
# init submodules here.
if [ ! -f "$AGENT_SRC/CMakeLists.txt" ]; then
    echo "Error: Micro-XRCE-DDS Agent not provisioned." >&2
    echo "  Run:  nros setup native --rmw xrce   (installs the prebuilt agent)" >&2
    echo "  Or, to build from source, first check out the submodule:" >&2
    echo "        git submodule update --init --recursive third-party/xrce/agent" >&2
    exit 1
fi

echo "Building Micro-XRCE-DDS Agent..."
echo "  Source: $AGENT_SRC"
echo "  Output: $BUILD_DIR/MicroXRCEAgent"
echo ""

# Configure and build
mkdir -p "$BUILD_DIR"
cd "$BUILD_DIR"
cmake "$AGENT_SRC" \
    -DUAGENT_BUILD_EXECUTABLE=ON \
    -DUAGENT_P2P_PROFILE=OFF \
    -DUAGENT_LOGGER_PROFILE=OFF \
    -DCMAKE_BUILD_TYPE=Release

cmake --build . --parallel "$(nproc 2>/dev/null || echo 4)"

# Verify
if [ ! -f "$BUILD_DIR/MicroXRCEAgent" ]; then
    echo "Error: MicroXRCEAgent binary not found after build"
    exit 1
fi

echo ""
echo "Build complete!"
echo "  Binary: $BUILD_DIR/MicroXRCEAgent"
