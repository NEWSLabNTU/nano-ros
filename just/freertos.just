set working-directory := '..'

# Pinned versions
FREERTOS_KERNEL_TAG := "V11.2.0"
LWIP_TAG := "STABLE-2_2_1_RELEASE"

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

# Download FreeRTOS kernel and lwIP sources
setup:
    #!/usr/bin/env bash
    set -e
    echo "=== FreeRTOS + lwIP Setup ==="
    echo ""
    FREERTOS_DIR="$(pwd)/external/freertos-kernel"
    LWIP_DIR="$(pwd)/external/lwip"

    # --- FreeRTOS Kernel ---
    if [ -d "$FREERTOS_DIR/include" ]; then
        tag=$(cd "$FREERTOS_DIR" && git describe --tags --exact-match 2>/dev/null || echo "unknown")
        echo "FreeRTOS kernel already present at $FREERTOS_DIR (tag: $tag)"
        if [ "$tag" != "{{FREERTOS_KERNEL_TAG}}" ]; then
            echo "  WARNING: expected {{FREERTOS_KERNEL_TAG}}, found $tag"
            echo "  To update: rm -rf $FREERTOS_DIR && just freertos setup"
        fi
    else
        echo "Cloning FreeRTOS kernel {{FREERTOS_KERNEL_TAG}}..."
        git clone --depth 1 --branch "{{FREERTOS_KERNEL_TAG}}" \
            https://github.com/FreeRTOS/FreeRTOS-Kernel.git "$FREERTOS_DIR"
        echo "  -> $FREERTOS_DIR"
    fi
    echo ""

    # --- lwIP ---
    if [ -d "$LWIP_DIR/src/include" ]; then
        tag=$(cd "$LWIP_DIR" && git describe --tags --exact-match 2>/dev/null || echo "unknown")
        echo "lwIP already present at $LWIP_DIR (tag: $tag)"
        if [ "$tag" != "{{LWIP_TAG}}" ]; then
            echo "  WARNING: expected {{LWIP_TAG}}, found $tag"
            echo "  To update: rm -rf $LWIP_DIR && just freertos setup"
        fi
    else
        echo "Cloning lwIP {{LWIP_TAG}}..."
        git clone --depth 1 --branch "{{LWIP_TAG}}" \
            https://github.com/lwip-tcpip/lwip.git "$LWIP_DIR"
        echo "  -> $LWIP_DIR"
    fi
    echo ""

    echo "=== Environment Variables ==="
    echo ""
    echo "Add these to your shell or .cargo/config.toml [env] section:"
    echo ""
    echo "  export FREERTOS_DIR=$FREERTOS_DIR"
    echo "  export FREERTOS_PORT=GCC/ARM_CM3"
    echo "  export LWIP_DIR=$LWIP_DIR"
    echo "  export FREERTOS_CONFIG_DIR=<board-crate>/config"
    echo ""
    echo "For QEMU MPS2-AN385 (Cortex-M3), use FREERTOS_PORT=GCC/ARM_CM3."
    echo "For STM32F7 (Cortex-M7), use FREERTOS_PORT=GCC/ARM_CM7/r0p1."
    echo ""
    echo "Setup complete!"

# Clean FreeRTOS build artifacts
clean:
    #!/usr/bin/env bash
    rm -rf build/cmake-freertos-armcm3-zenoh
    for example in talker listener service-server service-client action-server action-client; do
        rm -rf "examples/qemu-arm-freertos/rust/zenoh/$example/target"
    done
    echo "FreeRTOS build artifacts cleaned"
