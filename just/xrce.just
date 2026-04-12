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
