set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Install ThreadX Linux prerequisites: initialize submodules + build examples.
setup:
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$THREADX_DIR/common/inc" ] || [ ! -d "$NETX_DIR/common/inc" ]; then
        echo "Initializing ThreadX + NetX Duo submodules..."
        git submodule update --init --recursive third-party/threadx
    fi
    just threadx_linux build

# Diagnose ThreadX Linux install status (read-only).
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    if [ -d "$THREADX_DIR/common/inc" ]; then
        echo "  [OK] ThreadX kernel: $THREADX_DIR"
    else
        echo "  [MISSING] ThreadX kernel at $THREADX_DIR (run: just threadx_linux setup)"
        fail=1
    fi
    if [ -d "$NETX_DIR/common/inc" ]; then
        echo "  [OK] NetX Duo: $NETX_DIR"
    else
        echo "  [MISSING] NetX Duo at $NETX_DIR (run: just threadx_linux setup)"
        fail=1
    fi
    if [ -f examples/threadx-linux/rust/zenoh/talker/target/release/threadx-linux-talker ]; then
        echo "  [OK] example binaries built"
    else
        echo "  [MISSING] ThreadX Linux example binaries (run: just threadx_linux setup)"
        fail=1
    fi
    exit $fail

# Install ThreadX Linux libraries into build/install/
install:
    #!/usr/bin/env bash
    set -e
    if [ ! -d "${THREADX_DIR:-third-party/threadx/kernel}/common/inc" ]; then
        echo "ThreadX SDK not found — skipping ThreadX Linux libraries"
        exit 0
    fi
    PREFIX="$(pwd)/build/install"
    echo "=== Building threadx_linux RMW=zenoh ==="
    cmake -S . -B "build/cmake-threadx-linux-zenoh" \
        -DNANO_ROS_RMW="zenoh" \
        -DNANO_ROS_PLATFORM="threadx_linux" \
        -DNANO_ROS_BUILD_CODEGEN=OFF \
        -DCMAKE_BUILD_TYPE=Release
    cmake --build "build/cmake-threadx-linux-zenoh"
    cmake --install "build/cmake-threadx-linux-zenoh" --prefix "$PREFIX"

# Build ThreadX Linux examples
build:
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$THREADX_DIR/common/inc" ] || [ ! -d "$NETX_DIR/common/inc" ]; then
        echo "Skipping ThreadX Linux examples (ThreadX/NetX not found). Run: just threadx-linux setup"
        exit 0
    fi
    echo "Building ThreadX Linux examples..."
    for example in talker listener service-server service-client action-server action-client; do
        echo "  Building $example..."
        (cd examples/threadx-linux/rust/zenoh/$example && cargo build --release)
    done
    echo "ThreadX Linux examples built!"

# Run ThreadX Linux integration tests
test verbose="":
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$THREADX_DIR/common/inc" ]; then
        echo "ERROR: ThreadX not found at $THREADX_DIR. Run: just threadx-linux setup"
        exit 1
    fi
    if [ ! -d "$NETX_DIR/common/inc" ]; then
        echo "ERROR: NetX Duo not found at $NETX_DIR. Run: just threadx-linux setup"
        exit 1
    fi
    args=(-p nros-tests --test threadx_linux --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# =============================================================================
# Run examples (native Linux binaries)
#
# The ThreadX Linux network driver uses /dev/net/tun to attach to persistent
# user-owned TAP interfaces created by `just setup-network`. No CAP_NET_RAW
# or other file capabilities are needed.
# =============================================================================

# Helper: build and run.
_run example:
    #!/usr/bin/env bash
    set -e
    cd "examples/threadx-linux/rust/zenoh/{{example}}"
    cargo build --release
    exec target/release/threadx-linux-{{example}}

# Run ThreadX Linux talker
talker: (_run "talker")

# Run ThreadX Linux listener
listener: (_run "listener")

# Run ThreadX Linux service server
service-server: (_run "service-server")

# Run ThreadX Linux service client
service-client: (_run "service-client")

# Run ThreadX Linux action server
action-server: (_run "action-server")

# Run ThreadX Linux action client
action-client: (_run "action-client")

# Start zenohd router for ThreadX Linux examples (port 7455, bridge-networked)
zenohd:
    build/zenohd/zenohd --listen tcp/0.0.0.0:7455 --no-multicast-scouting

# Clean ThreadX Linux build artifacts
clean:
    #!/usr/bin/env bash
    rm -rf build/cmake-threadx-linux-zenoh
    for example in talker listener service-server service-client action-server action-client; do
        rm -rf "examples/threadx-linux/rust/zenoh/$example/target"
    done
    echo "ThreadX Linux build artifacts cleaned"
