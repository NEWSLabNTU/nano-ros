set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Install XRCE prerequisites: build Micro-XRCE-DDS Agent from source.
setup:
    #!/usr/bin/env bash
    set -e
    if [ -x build/xrce-agent/MicroXRCEAgent ]; then
        echo "Micro-XRCE-DDS Agent already built: build/xrce-agent/MicroXRCEAgent"
    else
        ./scripts/xrce-agent/build.sh
    fi

# Diagnose XRCE install status (read-only).
doctor:
    #!/usr/bin/env bash
    set +e
    if [ -x build/xrce-agent/MicroXRCEAgent ]; then
        echo "  [OK] Micro-XRCE-DDS Agent: build/xrce-agent/MicroXRCEAgent"
        exit 0
    else
        echo "  [MISSING] Micro-XRCE-DDS Agent (run: just xrce setup)"
        exit 1
    fi

# Build Micro-XRCE-DDS Agent from source (alias for `just xrce setup`).
build-agent:
    ./scripts/xrce-agent/build.sh

# Clean XRCE Agent build
clean-agent:
    ./scripts/xrce-agent/build.sh --clean

# Phase 115.K.2.5.2: build the standalone `nros-rmw-xrce-c` CMake
# project and install into the shared `build/install/` prefix so
# downstream `find_package(NrosRmwXrceC)` resolves it. Mirrors the
# cyclonedds `build-rmw` recipe — runs ahead of `install-local-posix`'s
# `xrce` iteration so the nros-c xrce variant links cleanly against
# `NrosRmwXrceC::NrosRmwXrceC`.
build-rmw:
    #!/usr/bin/env bash
    set -e
    PROJ="packages/xrce/nros-rmw-xrce-c"
    cmake -S "$PROJ" -B "$PROJ/build" \
        -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_PREFIX_PATH="$PWD/build/install" \
        -DCMAKE_INSTALL_PREFIX="$PWD/build/install" \
        -DNROS_RMW_XRCE_C_BUILD_TESTS=OFF
    cmake --build "$PROJ/build" --parallel
    cmake --install "$PROJ/build"

# Run XRCE-DDS integration tests (requires: just xrce build-agent)
test verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --no-fail-fast -E 'binary(xrce)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# XRCE ↔ ROS 2 DDS interop tests (needs XRCE Agent + ROS 2 + rmw_fastrtps)
test-ros2 verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --no-fail-fast -E 'binary(xrce_ros2_interop)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# C XRCE-DDS API integration tests (needs cmake + XRCE Agent)
test-c verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --no-fail-fast -E 'binary(c_xrce_api)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Clean XRCE build artifacts
clean: clean-agent
