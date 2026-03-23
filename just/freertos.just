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
