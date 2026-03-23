set working-directory := '..'

# Pinned ThreadX version
THREADX_TAG := "v6.4.5.202504_rel"

# Install ThreadX Linux libraries into build/install/
install:
    #!/usr/bin/env bash
    set -e
    if [ ! -d "${THREADX_DIR:-external/threadx}/common/inc" ]; then
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

# Download ThreadX kernel, NetX Duo, and learn-samples
setup:
    #!/usr/bin/env bash
    set -e
    echo "=== ThreadX + NetX Duo Setup ==="
    echo ""
    THREADX_DIR="$(pwd)/external/threadx"
    NETX_DIR="$(pwd)/external/netxduo"
    THREADX_LEARN_DIR="$(pwd)/external/threadx-learn-samples"

    # --- ThreadX Kernel ---
    if [ -d "$THREADX_DIR/common" ]; then
        tag=$(cd "$THREADX_DIR" && git describe --tags --exact-match 2>/dev/null || echo "unknown")
        echo "ThreadX already present at $THREADX_DIR (tag: $tag)"
        if [ "$tag" != "{{THREADX_TAG}}" ]; then
            echo "  WARNING: expected {{THREADX_TAG}}, found $tag"
            echo "  To update: rm -rf $THREADX_DIR && just threadx-linux setup"
        fi
    else
        echo "Cloning ThreadX {{THREADX_TAG}}..."
        git clone --depth 1 --branch "{{THREADX_TAG}}" \
            https://github.com/eclipse-threadx/threadx.git "$THREADX_DIR"
        echo "  -> $THREADX_DIR"
    fi
    echo ""

    # --- NetX Duo ---
    if [ -d "$NETX_DIR/common" ]; then
        tag=$(cd "$NETX_DIR" && git describe --tags --exact-match 2>/dev/null || echo "unknown")
        echo "NetX Duo already present at $NETX_DIR (tag: $tag)"
        if [ "$tag" != "{{THREADX_TAG}}" ]; then
            echo "  WARNING: expected {{THREADX_TAG}}, found $tag"
            echo "  To update: rm -rf $NETX_DIR && just threadx-linux setup"
        fi
    else
        echo "Cloning NetX Duo {{THREADX_TAG}}..."
        git clone --depth 1 --branch "{{THREADX_TAG}}" \
            https://github.com/eclipse-threadx/netxduo.git "$NETX_DIR"
        echo "  -> $NETX_DIR"
    fi
    echo ""

    # --- ThreadX Learn Samples (contains nx_linux_network_driver.c) ---
    if [ -d "$THREADX_LEARN_DIR/courses" ]; then
        echo "ThreadX learn-samples already present at $THREADX_LEARN_DIR"
    else
        echo "Cloning ThreadX learn-samples (main branch)..."
        git clone --depth 1 \
            https://github.com/eclipse-threadx/threadx-learn-samples.git "$THREADX_LEARN_DIR"
        echo "  -> $THREADX_LEARN_DIR"
    fi
    echo ""

    echo "=== Environment Variables ==="
    echo ""
    echo "Add these to your shell or .cargo/config.toml [env] section:"
    echo ""
    echo "  export THREADX_DIR=$THREADX_DIR"
    echo "  export THREADX_CONFIG_DIR=<board-crate>/config"
    echo "  export NETX_DIR=$NETX_DIR"
    echo "  export NETX_CONFIG_DIR=<board-crate>/config"
    echo ""
    echo "The Linux network driver is at:"
    echo "  $THREADX_LEARN_DIR/courses/netxduo/Driver/nx_linux_network_driver.c"
    echo ""
    echo "Setup complete!"

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
