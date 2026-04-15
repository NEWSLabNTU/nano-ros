set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Install FreeRTOS prerequisites: initialize the FreeRTOS + lwIP submodules.
setup:
    #!/usr/bin/env bash
    set -e
    if [ -d "$FREERTOS_DIR/include" ] && [ -d "$LWIP_DIR/src/include" ]; then
        echo "FreeRTOS + lwIP already present at $FREERTOS_DIR and $LWIP_DIR"
    else
        echo "Initializing FreeRTOS + lwIP submodules..."
        git submodule update --init --recursive third-party/freertos
    fi

# Diagnose FreeRTOS install status (read-only).
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    if [ -d "$FREERTOS_DIR/include" ]; then
        echo "  [OK] FreeRTOS kernel: $FREERTOS_DIR"
    else
        echo "  [MISSING] FreeRTOS kernel at $FREERTOS_DIR (run: just freertos setup)"
        fail=1
    fi
    if [ -d "$LWIP_DIR/src/include" ]; then
        echo "  [OK] lwIP: $LWIP_DIR"
    else
        echo "  [MISSING] lwIP at $LWIP_DIR (run: just freertos setup)"
        fail=1
    fi
    if command -v arm-none-eabi-gcc &>/dev/null; then
        echo "  [OK] arm-none-eabi-gcc"
    else
        echo "  [MISSING] arm-none-eabi-gcc (run: just workspace apt-packages)"
        fail=1
    fi
    exit $fail

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

# Run all FreeRTOS tests (same as test — no additional slow tests)
test-all verbose="": (test verbose)

# CI: check prerequisites + test
ci: doctor test

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
#
# Each example gets its own subdirectory so multiple instances can run in
# parallel (e.g., talker + listener with a shared zenohd).
#
# Usage:
#   just freertos trace talker    # single example
#   # For a pair, start zenohd first, then run in separate terminals:
#   #   terminal 1: build/zenohd/zenohd -l tcp/127.0.0.1:7451 --no-multicast-scouting
#   #   terminal 2: just freertos trace talker
#   #   terminal 3: just freertos trace listener
#
# Output: test-logs/freertos-trace/<example>/trace.pf
trace example="talker":
    #!/usr/bin/env bash
    set -e

    EXAMPLE="{{example}}"
    EXAMPLE_DIR="examples/qemu-arm-freertos/rust/zenoh/$EXAMPLE"
    TRACE_DIR="test-logs/freertos-trace/$EXAMPLE"
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

    # Build tband-cli if needed (before cd to trace dir)
    TBAND_CLI="third-party/tracing/Tonbandgeraet/tools/target/release/tband-cli"
    if [ ! -f "$TBAND_CLI" ]; then
        echo "Building tband-cli..."
        (cd "third-party/tracing/Tonbandgeraet/tools" && cargo build --release -p tband-cli)
    fi

    echo "=== Running $EXAMPLE in QEMU (with tracing) ==="
    echo "    Trace will be dumped to $TRACE_DIR/trace.bin on exit."
    # Run QEMU — semihosting creates trace.bin in CWD
    TRACE_DIR_ABS="$(pwd)/$TRACE_DIR"
    BINARY_ABS="$(pwd)/$BINARY_PATH"
    TBAND_CLI_ABS="$(pwd)/$TBAND_CLI"
    cd "$TRACE_DIR_ABS"
    rm -f trace.bin trace.pf

    # Run QEMU with a trap so Ctrl-C kills it cleanly.
    # QEMU with semihosting ignores SIGINT — we must kill explicitly.
    QEMU_PID=""
    trap 'if [ -n "$QEMU_PID" ]; then kill $QEMU_PID 2>/dev/null; wait $QEMU_PID 2>/dev/null; fi; exit 130' INT TERM
    timeout 60 qemu-system-arm \
        -cpu cortex-m3 -machine mps2-an385 -nographic \
        -icount shift=auto \
        -semihosting-config enable=on,target=native \
        -kernel "$BINARY_ABS" \
        -nic user,model=lan9118 &
    QEMU_PID=$!
    wait $QEMU_PID || true
    QEMU_PID=""
    trap - INT TERM

    if [ ! -f trace.bin ]; then
        echo "ERROR: trace.bin not created — semihosting file I/O may not be working"
        exit 1
    fi

    echo "=== Converting trace to Perfetto format ==="
    "$TBAND_CLI_ABS" conv --format bin --mode free-rtos --core-count 1 \
        --output trace.pf trace.bin

    echo ""
    echo "=== Trace captured ==="
    echo "  Raw:      $TRACE_DIR/trace.bin ($(wc -c < trace.bin) bytes)"
    echo "  Perfetto: $TRACE_DIR/trace.pf"
    echo ""
    echo "Open in browser: https://ui.perfetto.dev"
    echo "  → Open trace file → select $TRACE_DIR/trace.pf"

# =============================================================================
# Run examples (build + launch QEMU MPS2-AN385 with slirp networking)
#
# QEMU with semihosting ignores SIGINT (Ctrl-C). Each recipe runs QEMU in
# the background and traps INT/TERM to kill it explicitly.
# =============================================================================

[no-exit-message]
_run-qemu example binary:
    #!/usr/bin/env bash
    set -e
    cd "examples/qemu-arm-freertos/rust/zenoh/{{example}}" && cargo build --release
    trap 'kill $PID 2>/dev/null; wait $PID 2>/dev/null; exit 130' INT TERM
    qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic \
        -icount shift=auto \
        -semihosting-config enable=on,target=native \
        -kernel "target/thumbv7m-none-eabi/release/{{binary}}" \
        -nic user,model=lan9118 &
    PID=$!
    wait $PID

# Run FreeRTOS talker on QEMU
talker: (_run-qemu "talker" "qemu-freertos-talker")

# Run FreeRTOS listener on QEMU
listener: (_run-qemu "listener" "qemu-freertos-listener")

# Run FreeRTOS service server on QEMU
service-server: (_run-qemu "service-server" "qemu-freertos-service-server")

# Run FreeRTOS service client on QEMU
service-client: (_run-qemu "service-client" "qemu-freertos-service-client")

# Run FreeRTOS action server on QEMU
action-server: (_run-qemu "action-server" "qemu-freertos-action-server")

# Run FreeRTOS action client on QEMU
action-client: (_run-qemu "action-client" "qemu-freertos-action-client")

# Start zenohd router for FreeRTOS QEMU examples (port 7451)
zenohd:
    build/zenohd/zenohd --listen tcp/127.0.0.1:7451 --no-multicast-scouting
