set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}
# Zephyr workspace path (symlink or sibling directory)
ZEPHYR_WORKSPACE := if path_exists("zephyr-workspace") == "true" { "zephyr-workspace" } else { "../nano-ros-workspace" }

# Install the Zephyr workspace as a sibling directory (large download).
# Idempotent: skips if ../nano-ros-workspace/zephyr already exists.
# Extra args are forwarded: e.g. `just zephyr setup --force --skip-sdk`.
setup *ARGS:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    ARGS="{{ARGS}}"
    if [ -d "$WORKSPACE/zephyr" ] && [[ "$ARGS" != *--force* ]]; then
        echo "Zephyr workspace already present at $WORKSPACE"
        echo "To reinstall, run: just zephyr setup --force"
        # Re-apply Cortex-A9 patches anyway: idempotent, and ensures
        # workspaces created before the patch script existed get the
        # tweaks without needing --force (which re-downloads ~1 GB).
        bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$WORKSPACE"
    else
        ./scripts/zephyr/setup.sh $ARGS
    fi

# Diagnose Zephyr install status (read-only).
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if command -v west &>/dev/null; then
        echo "  [OK] west"
    else
        echo "  [MISSING] west (run: just zephyr setup)"
        fail=1
    fi
    if [ -d "$WORKSPACE/zephyr" ]; then
        echo "  [OK] Zephyr workspace: $WORKSPACE"
    else
        echo "  [MISSING] Zephyr workspace at $WORKSPACE (run: just zephyr setup)"
        fail=1
    fi
    # native_sim uses NSOS (host loopback) — no TAP bridge required.
    exit $fail

# Build Zephyr Rust examples (all Rust examples for native_sim)
build:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if [ ! -d "$WORKSPACE/zephyr" ]; then
        echo "Error: Zephyr workspace not found at $WORKSPACE"
        echo "Run: ./scripts/zephyr/setup.sh"
        exit 1
    fi
    # Apply Cortex-A9 patches (idempotent; no-op if already applied).
    bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$WORKSPACE"
    echo "Building Zephyr Rust examples in $WORKSPACE..."
    NROS="$(basename "$(pwd)")"
    cd "$WORKSPACE"
    for ex in talker listener service-server service-client action-server action-client async-service-client; do
        echo "  Building zephyr/rust/zenoh/$ex -> build-$ex/"
        west build -b native_sim/native/64 -d "build-$ex" -p auto "$NROS/examples/zephyr/rust/zenoh/$ex"
    done
    echo "Zephyr Rust examples built successfully!"

# Build all example binaries used as test fixtures (Rust + C + C++).
build-fixtures:
    #!/usr/bin/env bash
    set -e
    if ! command -v west >/dev/null 2>&1; then
        echo "Zephyr skip: west not found"; exit 0
    fi
    workspace="$(realpath ../nano-ros-workspace 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr skip: ../nano-ros-workspace not set up (run \`just zephyr setup\`)"
        exit 0
    fi
    # Apply Cortex-A9 patches (idempotent; required for the
    # build-dds-a9-{talker,listener} entries below).
    bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$workspace"
    echo "Building Zephyr test fixtures..."
    cmake_pfx="$(pwd)/build/install"
    rust_examples=(talker listener service-server service-client action-server action-client)
    for ex in "${rust_examples[@]}"; do
        echo "  → zephyr/rust/$ex"
        ( cd "$workspace" && \
          west build -b native_sim/native/64 -d "build-$ex" -p auto \
            "$(pwd)/../nano-ros/examples/zephyr/rust/zenoh/$ex" \
            -- -DCMAKE_PREFIX_PATH="$cmake_pfx" > /dev/null )
    done
    for ex in talker listener; do
        echo "  → zephyr/cpp/$ex"
        ( cd "$workspace" && \
          west build -b native_sim/native/64 -d "build-cpp-$ex" -p auto \
            "$(pwd)/../nano-ros/examples/zephyr/cpp/zenoh/$ex" \
            -- -DCMAKE_PREFIX_PATH="$cmake_pfx" > /dev/null )
    done
    for ex in service-server service-client action-server action-client; do
        echo "  → zephyr/cpp/$ex"
        ( cd "$workspace" && \
          west build -b native_sim/native/64 -d "build-cpp-$ex" -p auto \
            "$(pwd)/../nano-ros/examples/zephyr/cpp/zenoh/$ex" \
            -- -DCMAKE_PREFIX_PATH="$cmake_pfx" > /dev/null ) || true
    done
    # Zephyr XRCE-DDS examples (talker + listener for both Rust and C).
    # Build dirs match `nros_tests::zephyr::build_dir_for_example`.
    for ex in talker listener; do
        echo "  → zephyr/rust/xrce/$ex"
        ( cd "$workspace" && \
          west build -b native_sim/native/64 -d "build-xrce-rs-$ex" -p auto \
            "$(pwd)/../nano-ros/examples/zephyr/rust/xrce/$ex" \
            -- -DCMAKE_PREFIX_PATH="$cmake_pfx" > /dev/null )
    done
    for ex in talker listener; do
        echo "  → zephyr/c/xrce/$ex"
        ( cd "$workspace" && \
          west build -b native_sim/native/64 -d "build-xrce-c-$ex" -p auto \
            "$(pwd)/../nano-ros/examples/zephyr/c/xrce/$ex" \
            -- -DCMAKE_PREFIX_PATH="$cmake_pfx" > /dev/null )
    done
    # Phase 71.8 — Zephyr DDS Rust talker + listener (native_sim, boot smoke).
    # Build dirs match `nros_tests::zephyr::build_dir_for_example`.
    for ex in talker listener; do
        echo "  → zephyr/rust/dds/$ex (native_sim)"
        ( cd "$workspace" && \
          west build -b native_sim/native/64 -d "build-dds-rs-$ex" -p auto \
            "$(pwd)/../nano-ros/examples/zephyr/rust/dds/$ex" \
            -- -DCMAKE_PREFIX_PATH="$cmake_pfx" > /dev/null ) || true
    done
    # Phase 92 — Zephyr DDS Rust talker + listener (qemu_cortex_a9,
    # real IGMP / GEM / ARP). cortex-a9-rust-patch.sh above guarantees
    # the workspace has the zephyr-lang-rust + Zynq SoC patches; failure
    # here means a real regression.
    for ex in talker listener; do
        echo "  → zephyr/rust/dds/$ex (qemu_cortex_a9)"
        ( cd "$workspace" && \
          west build -b qemu_cortex_a9 -d "build-dds-a9-$ex" -p auto \
            "$(pwd)/../nano-ros/examples/zephyr/rust/dds/$ex" \
            -- -DCMAKE_PREFIX_PATH="$cmake_pfx" > /dev/null )
    done

# Build Zephyr C examples
build-c:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if [ ! -d "$WORKSPACE/zephyr" ]; then
        echo "Error: Zephyr workspace not found at $WORKSPACE"
        echo "Run: ./scripts/zephyr/setup.sh"
        exit 1
    fi
    echo "Building Zephyr C examples in $WORKSPACE..."
    NROS="$(basename "$(pwd)")"
    cd "$WORKSPACE"
    echo "  Building zephyr/c/zenoh/talker -> build-c-talker/"
    west build -b native_sim/native/64 -d build-c-talker -p auto $NROS/examples/zephyr/c/zenoh/talker
    echo "  Building zephyr/c/zenoh/listener -> build-c-listener/"
    west build -b native_sim/native/64 -d build-c-listener -p auto $NROS/examples/zephyr/c/zenoh/listener
    echo "Zephyr C examples built successfully!"

# Build Zephyr C++ examples
build-cpp:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if [ ! -d "$WORKSPACE/zephyr" ]; then
        echo "Error: Zephyr workspace not found at $WORKSPACE"
        echo "Run: ./scripts/zephyr/setup.sh"
        exit 1
    fi
    echo "Building Zephyr C++ examples in $WORKSPACE..."
    NROS="$(basename "$(pwd)")"
    cd "$WORKSPACE"
    for ex in talker listener service-server service-client action-server action-client; do
        echo "  Building zephyr/cpp/zenoh/$ex -> build-cpp-$ex/"
        west build -b native_sim/native/64 -d "build-cpp-$ex" -p auto "$NROS/examples/zephyr/cpp/zenoh/$ex"
    done
    echo "Zephyr C++ examples built successfully!"

# Build Zephyr XRCE examples (Rust + C for XRCE-DDS backend)
build-xrce:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if [ ! -d "$WORKSPACE/zephyr" ]; then
        echo "Error: Zephyr workspace not found at $WORKSPACE"
        echo "Run: ./scripts/zephyr/setup.sh"
        exit 1
    fi
    echo "Building Zephyr XRCE examples in $WORKSPACE..."
    NROS="$(basename "$(pwd)")"
    cd "$WORKSPACE"
    echo "  Building zephyr/rust/xrce/talker -> build-xrce-rs-talker/"
    west build -b native_sim/native/64 -d build-xrce-rs-talker -p auto $NROS/examples/zephyr/rust/xrce/talker
    echo "  Building zephyr/rust/xrce/listener -> build-xrce-rs-listener/"
    west build -b native_sim/native/64 -d build-xrce-rs-listener -p auto $NROS/examples/zephyr/rust/xrce/listener
    echo "  Building zephyr/c/xrce/talker -> build-xrce-c-talker/"
    west build -b native_sim/native/64 -d build-xrce-c-talker -p auto $NROS/examples/zephyr/c/xrce/talker
    echo "  Building zephyr/c/xrce/listener -> build-xrce-c-listener/"
    west build -b native_sim/native/64 -d build-xrce-c-listener -p auto $NROS/examples/zephyr/c/xrce/listener
    echo "Zephyr XRCE examples built successfully!"

# Build all Zephyr examples (Rust + C + C++ + XRCE)
build-all: build build-c build-cpp build-xrce
    @echo "All Zephyr examples built!"

# Run Zephyr E2E tests (requires pre-built Zephyr examples; native_sim uses NSOS on host loopback)
test verbose="":
    #!/usr/bin/env bash
    set -e
    just build-zenohd
    args=(-p nros-tests --test zephyr --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    else
        args+=(--success-output immediate --failure-output immediate)
    fi
    cargo nextest run "${args[@]}"

# Run all Zephyr tests (zenoh + XRCE + C)
test-all verbose="":
    #!/usr/bin/env bash
    set +e
    failed=0
    just zephyr test {{verbose}} || failed=1
    just zephyr test-xrce {{verbose}} || failed=1
    just zephyr test-c || failed=1
    if [ $failed -ne 0 ]; then
        echo "FAIL: Some Zephyr tests failed."
        exit 1
    fi

# CI: doctor + test
ci: doctor test

# Run Zephyr tests with full rebuild
test-full verbose="": build
    just zephyr test {{verbose}}

# Run Zephyr XRCE E2E tests
test-xrce verbose="":
    #!/usr/bin/env bash
    set -e
    args=(-p nros-tests --test zephyr --no-fail-fast -E 'test(xrce)')
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    else
        args+=(--success-output immediate --failure-output immediate)
    fi
    cargo nextest run "${args[@]}"

# Run Zephyr C examples test
test-c:
    just build-zenohd
    ./tests/zephyr/run-c.sh

# Clean Zephyr build directories
clean:
    #!/usr/bin/env bash
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    rm -rf "$WORKSPACE"/build-{talker,listener,service-server,service-client,action-server,action-client,async-service-client,c-talker,c-listener,cpp-talker,cpp-listener,cpp-service-server,cpp-service-client,cpp-action-server,cpp-action-client,xrce-rs-talker,xrce-rs-listener,xrce-c-talker,xrce-c-listener}
    echo "Zephyr build directories cleaned"

# Force rebuild Zephyr examples
rebuild: clean build

# Show Zephyr help
help:
    @echo "Zephyr Examples"
    @echo "==============="
    @echo ""
    @echo "Prerequisites:"
    @echo "  Set up Zephyr workspace: ./scripts/zephyr/setup.sh"
    @echo "  (native_sim uses NSOS; no TAP bridge/sudo required.)"
    @echo ""
    @echo "Build examples:"
    @echo "  just zephyr build          # Build Rust zenoh Zephyr examples"
    @echo "  just zephyr build-c        # Build C zenoh Zephyr examples"
    @echo "  just zephyr build-cpp      # Build C++ zenoh Zephyr examples"
    @echo "  just zephyr build-xrce     # Build XRCE Zephyr examples (Rust + C)"
    @echo "  just zephyr build-all      # Build all Zephyr examples"
    @echo "  just zephyr rebuild        # Clean and rebuild"
    @echo "  just zephyr clean          # Remove all build directories"
    @echo ""
    @echo "Run tests:"
    @echo "  just zephyr test           # Run zenoh Zephyr E2E tests"
    @echo "  just zephyr test-xrce      # Run XRCE Zephyr E2E tests"
    @echo "  just zephyr test-full      # Rebuild and run zenoh Zephyr tests"
    @echo "  just zephyr test-c         # Run Zephyr C examples test"
    @echo ""
    @echo "Manual build (from Zephyr workspace):"
    @echo "  west build -b native_sim/native/64 -d build-talker <repo>/examples/zephyr/rust/zenoh/talker"
    @echo "  west build -b native_sim/native/64 -d build-listener <repo>/examples/zephyr/rust/zenoh/listener"

# =============================================================================
# Run examples (build + launch native_sim binary)
#
# Each recipe passes --seed=$RANDOM so that multiple native_sim instances
# get different Zenoh session IDs. Without unique seeds, deterministic
# entropy produces identical ZIDs and the router rejects the second
# session with Close(MAX_LINKS). On real SoCs this isn't needed — the
# hardware RNG provides unique entropy.
# =============================================================================

# Run Zephyr talker
talker:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    west build -b native_sim/native/64 -d build-talker -p auto "$NROS/examples/zephyr/rust/zenoh/talker"
    ./build-talker/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr listener
listener:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    west build -b native_sim/native/64 -d build-listener -p auto "$NROS/examples/zephyr/rust/zenoh/listener"
    ./build-listener/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr service server
service-server:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    west build -b native_sim/native/64 -d build-service-server -p auto "$NROS/examples/zephyr/rust/zenoh/service-server"
    ./build-service-server/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr service client
service-client:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    west build -b native_sim/native/64 -d build-service-client -p auto "$NROS/examples/zephyr/rust/zenoh/service-client"
    ./build-service-client/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr action server
action-server:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    west build -b native_sim/native/64 -d build-action-server -p auto "$NROS/examples/zephyr/rust/zenoh/action-server"
    ./build-action-server/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr action client
action-client:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    west build -b native_sim/native/64 -d build-action-client -p auto "$NROS/examples/zephyr/rust/zenoh/action-client"
    ./build-action-client/zephyr/zephyr.exe --seed=$RANDOM

# Run Zephyr async service client
async-service-client:
    #!/usr/bin/env bash
    set -e
    NROS="$(basename "$(pwd)")"
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    cd "$WORKSPACE"
    west build -b native_sim/native/64 -d build-async-service-client -p auto "$NROS/examples/zephyr/rust/zenoh/async-service-client"
    ./build-async-service-client/zephyr/zephyr.exe

# Start zenohd router for Zephyr examples (port 7456, all interfaces)
zenohd:
    build/zenohd/zenohd --listen tcp/0.0.0.0:7456 --no-multicast-scouting
