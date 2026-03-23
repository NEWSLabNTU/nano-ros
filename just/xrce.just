set working-directory := '..'
# Build Micro-XRCE-DDS Agent from source (for XRCE integration tests)
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
