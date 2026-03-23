set working-directory := '..'
# Install ThreadX RISC-V 64 libraries into build/install/
install:
    #!/usr/bin/env bash
    set -e
    if ! command -v riscv64-unknown-elf-gcc &>/dev/null || \
       [ ! -d "${THREADX_DIR:-external/threadx}/common/inc" ]; then
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

# Clean ThreadX RISC-V build artifacts
clean:
    #!/usr/bin/env bash
    rm -rf build/cmake-threadx-riscv64-zenoh
    for example in talker listener service-server service-client action-server action-client; do
        rm -rf "examples/qemu-riscv64-threadx/rust/zenoh/$example/target"
    done
    echo "ThreadX RISC-V build artifacts cleaned"
