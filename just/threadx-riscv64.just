set working-directory := '..'

default:
    @just --list --justfile {{source_file()}}

# Install ThreadX RISC-V 64 prerequisites: initialize submodules.
# Does NOT build example binaries — use `just threadx_riscv64 build` for that.
setup:
    #!/usr/bin/env bash
    set -e
    if [ -d "$THREADX_DIR/common/inc" ] && [ -d "$NETX_DIR/common/inc" ]; then
        echo "ThreadX + NetX Duo already present at $THREADX_DIR and $NETX_DIR"
    else
        echo "Initializing ThreadX + NetX Duo submodules..."
        git submodule update --init --recursive third-party/threadx
    fi

# Diagnose ThreadX RISC-V 64 install status (read-only).
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    if [ -d "$THREADX_DIR/common/inc" ]; then
        echo "  [OK] ThreadX kernel: $THREADX_DIR"
    else
        echo "  [MISSING] ThreadX kernel at $THREADX_DIR (run: just threadx_riscv64 setup)"
        fail=1
    fi
    if [ -d "$NETX_DIR/common/inc" ]; then
        echo "  [OK] NetX Duo: $NETX_DIR"
    else
        echo "  [MISSING] NetX Duo at $NETX_DIR (run: just threadx_riscv64 setup)"
        fail=1
    fi
    if command -v riscv64-unknown-elf-gcc &>/dev/null; then
        echo "  [OK] riscv64-unknown-elf-gcc"
    else
        echo "  [MISSING] riscv64-unknown-elf-gcc (run: just workspace apt-packages)"
        fail=1
    fi
    if command -v qemu-system-riscv64 &>/dev/null; then
        echo "  [OK] qemu-system-riscv64"
    else
        echo "  [MISSING] qemu-system-riscv64 (run: just workspace apt-packages)"
        fail=1
    fi
    exit $fail

# Install ThreadX RISC-V 64 libraries into build/install/
install:
    #!/usr/bin/env bash
    set -e
    if ! command -v riscv64-unknown-elf-gcc &>/dev/null || \
       [ ! -d "${THREADX_DIR:-third-party/threadx/kernel}/common/inc" ]; then
        echo "RISC-V toolchain or ThreadX SDK not found — skipping ThreadX RISC-V libraries"
        exit 0
    fi
    PREFIX="$(pwd)/build/install"
    rv_config="$(pwd)/packages/boards/nros-threadx-qemu-riscv64/config"
    echo "=== Building threadx_riscv64 RMW=zenoh ==="
    THREADX_CONFIG_DIR="$rv_config" NETX_CONFIG_DIR="$rv_config" \
    cmake -S . -B "build/cmake-threadx-riscv64-zenoh" \
        -DCMAKE_TOOLCHAIN_FILE="cmake/toolchain/riscv64-threadx.cmake" \
        -DNANO_ROS_RMW="zenoh" \
        -DNANO_ROS_PLATFORM="threadx_riscv64" \
        -DNANO_ROS_BUILD_CODEGEN=OFF \
        -DCMAKE_BUILD_TYPE=Release
    THREADX_CONFIG_DIR="$rv_config" NETX_CONFIG_DIR="$rv_config" \
    cmake --build "build/cmake-threadx-riscv64-zenoh"
    cmake --install "build/cmake-threadx-riscv64-zenoh" --prefix "$PREFIX"

# Build ThreadX QEMU RISC-V 64 examples
build:
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$THREADX_DIR/common/inc" ] || [ ! -d "$NETX_DIR/common/inc" ]; then
        echo "Skipping ThreadX QEMU RISC-V examples (ThreadX/NetX not found). Run: just threadx-linux setup"
        exit 0
    fi
    if ! command -v riscv64-unknown-elf-gcc &>/dev/null; then
        echo "Skipping ThreadX QEMU RISC-V examples (riscv64-unknown-elf-gcc not found)"
        exit 0
    fi
    echo "Building ThreadX QEMU RISC-V 64 examples..."
    rv_config="$(pwd)/packages/boards/nros-threadx-qemu-riscv64/config"
    for example in talker listener service-server service-client action-server action-client; do
        echo "  Building $example..."
        (cd examples/qemu-riscv64-threadx/rust/zenoh/$example && \
         THREADX_CONFIG_DIR="$rv_config" NETX_CONFIG_DIR="$rv_config" cargo build --release)
    done
    echo "ThreadX QEMU RISC-V examples built!"

# Run ThreadX QEMU RISC-V 64 integration tests
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
    rv_config="$(pwd)/packages/boards/nros-threadx-qemu-riscv64/config"
    export THREADX_CONFIG_DIR="$rv_config"
    export NETX_CONFIG_DIR="$rv_config"
    args=(-p nros-tests --test threadx_riscv64_qemu --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Run all ThreadX RISC-V tests (same as test)
test-all verbose="": (test verbose)

# CI: check prerequisites + test
ci: doctor test

# =============================================================================
# Run examples (build + launch QEMU RISC-V 64 virt with slirp networking)
#
# QEMU ignores SIGINT — each recipe runs QEMU in the background and
# traps INT/TERM to kill it explicitly (Ctrl-C works).
# peer_index: 0=talker/server (MAC :56), 1=listener/client (MAC :57)
# =============================================================================

[no-exit-message]
_run-qemu example binary peer_index:
    #!/usr/bin/env bash
    set -e
    rv_config="$(pwd)/packages/boards/nros-threadx-qemu-riscv64/config"
    cd "examples/qemu-riscv64-threadx/rust/zenoh/{{example}}"
    THREADX_CONFIG_DIR="$rv_config" NETX_CONFIG_DIR="$rv_config" cargo build --release
    MAC="52:54:00:12:34:$(printf '%02x' $((0x56 + {{peer_index}})))"
    trap 'kill $PID 2>/dev/null; wait $PID 2>/dev/null; exit 130' INT TERM
    qemu-system-riscv64 -M virt -m 256M -bios none -nographic \
        -global virtio-mmio.force-legacy=false \
        -kernel "target/riscv64gc-unknown-none-elf/release/{{binary}}" \
        -netdev user,id=net0 \
        -device "virtio-net-device,netdev=net0,bus=virtio-mmio-bus.0,mac=$MAC" &
    PID=$!
    wait $PID

# Run ThreadX RISC-V talker on QEMU
talker: (_run-qemu "talker" "qemu-riscv64-threadx-talker" "0")

# Run ThreadX RISC-V listener on QEMU
listener: (_run-qemu "listener" "qemu-riscv64-threadx-listener" "1")

# Run ThreadX RISC-V service server on QEMU
service-server: (_run-qemu "service-server" "qemu-riscv64-threadx-service-server" "0")

# Run ThreadX RISC-V service client on QEMU
service-client: (_run-qemu "service-client" "qemu-riscv64-threadx-service-client" "1")

# Run ThreadX RISC-V action server on QEMU
action-server: (_run-qemu "action-server" "qemu-riscv64-threadx-action-server" "0")

# Run ThreadX RISC-V action client on QEMU
action-client: (_run-qemu "action-client" "qemu-riscv64-threadx-action-client" "1")

# Start zenohd router for ThreadX RISC-V examples (port 7453)
zenohd:
    build/zenohd/zenohd --listen tcp/127.0.0.1:7453 --no-multicast-scouting

# Clean ThreadX RISC-V build artifacts
clean:
    #!/usr/bin/env bash
    rm -rf build/cmake-threadx-riscv64-zenoh
    for example in talker listener service-server service-client action-server action-client; do
        rm -rf "examples/qemu-riscv64-threadx/rust/zenoh/$example/target"
    done
    echo "ThreadX RISC-V build artifacts cleaned"
