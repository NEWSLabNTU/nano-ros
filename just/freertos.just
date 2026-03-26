set working-directory := '..'

# Install FreeRTOS ARM Cortex-M3 libraries into build/install/
install:
    #!/usr/bin/env bash
    set -e
    if ! command -v arm-none-eabi-gcc &>/dev/null; then
        echo "ERROR: arm-none-eabi-gcc not found"
        echo "Install with: sudo apt install gcc-arm-none-eabi"
        exit 1
    fi
    PREFIX="$(pwd)/build/install"
    echo "=== Building freertos_armcm3 RMW=zenoh ==="
    cmake -S . -B "build/cmake-freertos-armcm3-zenoh" \
        -DCMAKE_TOOLCHAIN_FILE="cmake/toolchain/arm-freertos-armcm3.cmake" \
        -DNANO_ROS_RMW="zenoh" \
        -DNANO_ROS_PLATFORM="freertos_armcm3" \
        -DNANO_ROS_BUILD_CODEGEN=OFF \
        -DCMAKE_BUILD_TYPE=Release
    cmake --build "build/cmake-freertos-armcm3-zenoh"
    cmake --install "build/cmake-freertos-armcm3-zenoh" --prefix "$PREFIX"

# Build FreeRTOS QEMU MPS2-AN385 Rust examples
build:
    #!/usr/bin/env bash
    set -e
    echo "Building FreeRTOS QEMU MPS2-AN385 examples..."
    if [ ! -d "$FREERTOS_DIR/include" ]; then
        echo "ERROR: FreeRTOS not found at $FREERTOS_DIR. Run: just setup-freertos"
        exit 1
    fi
    if [ ! -d "$LWIP_DIR/src/include" ]; then
        echo "ERROR: lwIP not found at $LWIP_DIR. Run: just setup-freertos"
        exit 1
    fi
    for example in talker listener service-server service-client action-server action-client; do
        echo "  Building $example..."
        (cd examples/qemu-arm-freertos/rust/zenoh/$example && cargo build --release)
    done
    echo "FreeRTOS QEMU examples built!"

# Run FreeRTOS QEMU integration tests
test verbose="":
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$FREERTOS_DIR/include" ]; then
        echo "ERROR: FreeRTOS not found at $FREERTOS_DIR. Run: just setup-freertos"
        exit 1
    fi
    if [ ! -d "$LWIP_DIR/src/include" ]; then
        echo "ERROR: lwIP not found at $LWIP_DIR. Run: just setup-freertos"
        exit 1
    fi
    args=(-p nros-tests --test freertos_qemu --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Clean FreeRTOS build artifacts
clean:
    #!/usr/bin/env bash
    rm -rf build/cmake-freertos-armcm3-zenoh
    for example in talker listener service-server service-client action-server action-client; do
        rm -rf "examples/qemu-arm-freertos/rust/zenoh/$example/target"
    done
    echo "FreeRTOS build artifacts cleaned"

# =============================================================================
# Tracing (Tonbandgeraet → Perfetto)
# =============================================================================

# Build and run a FreeRTOS example with tracing, then convert to Perfetto.
# Usage: just freertos trace [example]   (default: talker)
# Output: test-logs/freertos-trace/trace.pf (open with https://ui.perfetto.dev)
trace example="talker":
    #!/usr/bin/env bash
    set -e

    EXAMPLE="{{example}}"
    EXAMPLE_DIR="examples/qemu-arm-freertos/rust/zenoh/$EXAMPLE"
    TRACE_DIR="test-logs/freertos-trace"
    mkdir -p "$TRACE_DIR"

    if [ ! -d "$EXAMPLE_DIR" ]; then
        echo "ERROR: Example not found: $EXAMPLE_DIR"
        exit 1
    fi

    echo "=== Building $EXAMPLE with tracing ==="
    (cd "$EXAMPLE_DIR" && NROS_TRACE=1 cargo build --release)

    # Find the binary name from Cargo.toml
    BINARY=$(grep '^name = ' "$EXAMPLE_DIR/Cargo.toml" | head -1 | sed 's/name = "\(.*\)"/\1/')
    BINARY_PATH="$EXAMPLE_DIR/target/thumbv7m-none-eabi/release/$BINARY"
    if [ ! -f "$BINARY_PATH" ]; then
        echo "ERROR: Binary not found: $BINARY_PATH"
        exit 1
    fi

    echo "=== Running $EXAMPLE in QEMU (with tracing) ==="
    echo "    Trace will be dumped via semihosting on exit."
    # Run QEMU — semihosting creates trace.bin in CWD
    cd "$TRACE_DIR"
    timeout 60 qemu-system-arm \
        -cpu cortex-m3 -machine mps2-an385 -nographic \
        -icount shift=auto \
        -semihosting-config enable=on,target=native \
        -kernel "../../$BINARY_PATH" \
        -nic user,model=lan9118 || true

    if [ ! -f trace.bin ]; then
        echo "ERROR: trace.bin not created — semihosting file I/O may not be working"
        exit 1
    fi

    echo "=== Converting trace to Perfetto format ==="
    TBAND_CLI="../../third-party/tracing/Tonbandgeraet/tools/target/release/tband-cli"
    if [ ! -f "$TBAND_CLI" ]; then
        echo "Building tband-cli..."
        (cd "../../third-party/tracing/Tonbandgeraet/tools" && cargo build --release -p tband-cli)
    fi
    "$TBAND_CLI" conv --format bin --mode free-rtos --core-count 1 \
        --output trace.pf trace.bin

    echo ""
    echo "=== Trace captured ==="
    echo "  Raw:     $TRACE_DIR/trace.bin ($(wc -c < trace.bin) bytes)"
    echo "  Perfetto: $TRACE_DIR/trace.pf"
    echo ""
    echo "Open in browser: https://ui.perfetto.dev"
    echo "  → Open trace file → select $TRACE_DIR/trace.pf"

# =============================================================================
# Run examples (build + launch QEMU MPS2-AN385 with slirp networking)
# =============================================================================

# Run FreeRTOS talker on QEMU
talker:
    #!/usr/bin/env bash
    set -e
    cd examples/qemu-arm-freertos/rust/zenoh/talker && cargo build --release
    qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
        -icount shift=auto \
        -semihosting-config enable=on,target=native \
        -kernel target/thumbv7m-none-eabi/release/qemu-freertos-talker \
        -nic user,model=lan9118

# Run FreeRTOS listener on QEMU
listener:
    #!/usr/bin/env bash
    set -e
    cd examples/qemu-arm-freertos/rust/zenoh/listener && cargo build --release
    qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
        -icount shift=auto \
        -semihosting-config enable=on,target=native \
        -kernel target/thumbv7m-none-eabi/release/qemu-freertos-listener \
        -nic user,model=lan9118

# Run FreeRTOS service server on QEMU
service-server:
    #!/usr/bin/env bash
    set -e
    cd examples/qemu-arm-freertos/rust/zenoh/service-server && cargo build --release
    qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
        -icount shift=auto \
        -semihosting-config enable=on,target=native \
        -kernel target/thumbv7m-none-eabi/release/qemu-freertos-service-server \
        -nic user,model=lan9118

# Run FreeRTOS service client on QEMU
service-client:
    #!/usr/bin/env bash
    set -e
    cd examples/qemu-arm-freertos/rust/zenoh/service-client && cargo build --release
    qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
        -icount shift=auto \
        -semihosting-config enable=on,target=native \
        -kernel target/thumbv7m-none-eabi/release/qemu-freertos-service-client \
        -nic user,model=lan9118

# Run FreeRTOS action server on QEMU
action-server:
    #!/usr/bin/env bash
    set -e
    cd examples/qemu-arm-freertos/rust/zenoh/action-server && cargo build --release
    qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
        -icount shift=auto \
        -semihosting-config enable=on,target=native \
        -kernel target/thumbv7m-none-eabi/release/qemu-freertos-action-server \
        -nic user,model=lan9118

# Run FreeRTOS action client on QEMU
action-client:
    #!/usr/bin/env bash
    set -e
    cd examples/qemu-arm-freertos/rust/zenoh/action-client && cargo build --release
    qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
        -icount shift=auto \
        -semihosting-config enable=on,target=native \
        -kernel target/thumbv7m-none-eabi/release/qemu-freertos-action-client \
        -nic user,model=lan9118

# Start zenohd router for FreeRTOS QEMU examples (port 7451)
zenohd:
    build/zenohd/zenohd --listen tcp/127.0.0.1:7451 --no-multicast-scouting
