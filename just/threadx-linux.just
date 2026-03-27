set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

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

# Apply CAP_NET_RAW to ThreadX Linux binaries (requires sudo)
setup-caps: build
    #!/usr/bin/env bash
    set -e
    echo "=== Applying CAP_NET_RAW to ThreadX Linux binaries ==="
    echo ""
    EXAMPLES_DIR="examples/threadx-linux/rust/zenoh"
    found=0
    for dir in "$EXAMPLES_DIR"/*/; do
        name=$(basename "$dir")
        binary="$dir/target/release/threadx-linux-$name"
        if [ -f "$binary" ]; then
            echo "  setcap cap_net_raw+ep $binary"
            sudo setcap cap_net_raw+ep "$binary"
            found=$((found + 1))
        fi
    done
    if [ $found -eq 0 ]; then
        echo "No built ThreadX binaries found."
        exit 1
    fi
    echo ""
    echo "Applied capabilities to $found binaries."
    echo "ThreadX E2E tests should now pass with: just threadx-linux test"

# Clean ThreadX Linux build artifacts
clean:
    #!/usr/bin/env bash
    rm -rf build/cmake-threadx-linux-zenoh
    for example in talker listener service-server service-client action-server action-client; do
        rm -rf "examples/threadx-linux/rust/zenoh/$example/target"
    done
    echo "ThreadX Linux build artifacts cleaned"
