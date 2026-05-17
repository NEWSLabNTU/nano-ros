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

# Phase 140 — the legacy `build-rmw` recipe (standalone CMake build
# + install) is gone. The Rust-side `nros-rmw-xrce-cffi` shim is
# what the root CMake xrce branch consumes; build it via cargo or
# via the root cmake configure with NANO_ROS_RMW=xrce. The
# header-only `nros-rmw-xrce` C++ project under
# packages/xrce/nros-rmw-xrce/ keeps its CTest harness for ad-hoc
# in-tree validation.

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

# Phase 121.7.i — `cargo check` the out-of-workspace nros-rmw-xrce Rust
# crate so CI catches drift after platform-API or trait-surface changes.
# Workspace `cargo check` skips it (it's workspace-excluded for xrce-sys
# linking reasons), so this recipe is the only place it gets compiled.
check-rust-rmw:
    #!/usr/bin/env bash
    set -e
    cd packages/xrce/nros-rmw-xrce
    cargo check --quiet

# Clean XRCE build artifacts
clean: clean-agent
