set working-directory := '..'
# Zephyr workspace path (symlink or sibling directory)
ZEPHYR_WORKSPACE := if path_exists("zephyr-workspace") == "true" { "zephyr-workspace" } else { "../nano-ros-workspace" }

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
    echo "Building Zephyr Rust examples in $WORKSPACE..."
    cd "$WORKSPACE"
    for ex in talker listener service-server service-client action-server action-client async-service-client; do
        echo "  Building zephyr/rust/zenoh/$ex -> build-$ex/"
        west build -b native_sim/native/64 -d "build-$ex" -p auto "nros/examples/zephyr/rust/zenoh/$ex"
    done
    echo "Zephyr Rust examples built successfully!"

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
    cd "$WORKSPACE"
    echo "  Building zephyr/c/zenoh/talker -> build-c-talker/"
    west build -b native_sim/native/64 -d build-c-talker -p auto nros/examples/zephyr/c/zenoh/talker
    echo "  Building zephyr/c/zenoh/listener -> build-c-listener/"
    west build -b native_sim/native/64 -d build-c-listener -p auto nros/examples/zephyr/c/zenoh/listener
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
    cd "$WORKSPACE"
    for ex in talker listener service-server service-client action-server action-client; do
        echo "  Building zephyr/cpp/zenoh/$ex -> build-cpp-$ex/"
        west build -b native_sim/native/64 -d "build-cpp-$ex" -p auto "nros/examples/zephyr/cpp/zenoh/$ex"
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
    cd "$WORKSPACE"
    echo "  Building zephyr/rust/xrce/talker -> build-xrce-rs-talker/"
    west build -b native_sim/native/64 -d build-xrce-rs-talker -p auto nros/examples/zephyr/rust/xrce/talker
    echo "  Building zephyr/rust/xrce/listener -> build-xrce-rs-listener/"
    west build -b native_sim/native/64 -d build-xrce-rs-listener -p auto nros/examples/zephyr/rust/xrce/listener
    echo "  Building zephyr/c/xrce/talker -> build-xrce-c-talker/"
    west build -b native_sim/native/64 -d build-xrce-c-talker -p auto nros/examples/zephyr/c/xrce/talker
    echo "  Building zephyr/c/xrce/listener -> build-xrce-c-listener/"
    west build -b native_sim/native/64 -d build-xrce-c-listener -p auto nros/examples/zephyr/c/xrce/listener
    echo "Zephyr XRCE examples built successfully!"

# Build all Zephyr examples (Rust + C + C++ + XRCE)
build-all: build build-c build-cpp build-xrce
    @echo "All Zephyr examples built!"

# Run Zephyr E2E tests (requires pre-built Zephyr examples + bridge network)
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
    @echo "  1. Set up Zephyr workspace: ./scripts/zephyr/setup.sh"
    @echo "  2. Set up bridge network:   sudo ./scripts/zephyr/setup-network.sh"
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
    @echo "  west build -b native_sim/native/64 -d build-talker nros/examples/zephyr/rust/zenoh/talker"
    @echo "  west build -b native_sim/native/64 -d build-listener nros/examples/zephyr/rust/zenoh/listener"
