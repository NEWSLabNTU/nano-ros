set working-directory := '..'

# Install NuttX ARM libraries into build/install/
install:
    #!/usr/bin/env bash
    set -e
    NUTTX_EXPORT=$(ls -d "third-party/nuttx/nuttx/nuttx-export-"* 2>/dev/null | head -1)
    if ! command -v arm-none-eabi-gcc &>/dev/null || [ -z "$NUTTX_EXPORT" ]; then
        echo "NuttX export not found — skipping NuttX ARM libraries (run: just nuttx build-kernel)"
        exit 0
    fi
    PREFIX="$(pwd)/build/install"
    echo "=== Building nuttx_armv7a RMW=zenoh ==="
    cmake -S . -B "build/cmake-nuttx-armv7a-zenoh" \
        -DCMAKE_TOOLCHAIN_FILE="cmake/toolchain/armv7a-nuttx-eabi.cmake" \
        -DNANO_ROS_RMW="zenoh" \
        -DNANO_ROS_PLATFORM="nuttx_armv7a" \
        -DNANO_ROS_BUILD_CODEGEN=OFF \
        -DCMAKE_BUILD_TYPE=Release
    cmake --build "build/cmake-nuttx-armv7a-zenoh"
    cmake --install "build/cmake-nuttx-armv7a-zenoh" --prefix "$PREFIX"

# Build NuttX kernel (requires arm-none-eabi-gcc + kconfig tools)
build-kernel:
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$NUTTX_DIR/include" ]; then
        echo "Skipping NuttX kernel build (NuttX not found). Run: just nuttx setup"
        exit 0
    fi
    if ! command -v arm-none-eabi-gcc &>/dev/null; then
        echo "Skipping NuttX kernel build (arm-none-eabi-gcc not found)"
        exit 0
    fi
    if ! command -v kconfig-conf &>/dev/null && ! command -v olddefconfig &>/dev/null; then
        echo "Skipping NuttX kernel build (kconfig tools not found)"
        exit 0
    fi
    NUTTX_DIR=$NUTTX_DIR NUTTX_APPS_DIR=$NUTTX_APPS_DIR \
        packages/boards/nros-nuttx-qemu-arm/scripts/build-nuttx.sh

# Build NuttX QEMU ARM virt examples
build:
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$NUTTX_DIR/include" ]; then
        echo "Skipping NuttX QEMU examples (NuttX not found). Run: just nuttx setup"
        exit 0
    fi
    echo "Building NuttX QEMU ARM virt examples..."
    for example in talker listener service-server service-client action-server action-client; do
        echo "  Building $example..."
        (cd examples/qemu-arm-nuttx/rust/zenoh/$example && cargo +nightly build --release)
    done
    echo "NuttX QEMU examples built!"

# Run NuttX QEMU integration tests
test verbose="":
    #!/usr/bin/env bash
    set -e
    if [ ! -d "$NUTTX_DIR/include" ]; then
        echo "ERROR: NuttX not found at $NUTTX_DIR. Run: just nuttx setup"
        exit 1
    fi
    args=(-p nros-tests --test nuttx_qemu --no-fail-fast)
    if [ -z "{{verbose}}" ]; then
        args+=(--success-output never --failure-output never)
    fi
    cargo nextest run "${args[@]}"

# Build NuttX kernel (run after git submodule update --init)
setup: build-kernel
    @echo "NuttX setup complete!"

# Clean NuttX build artifacts
clean:
    #!/usr/bin/env bash
    rm -rf build/cmake-nuttx-armv7a-zenoh
    for example in talker listener service-server service-client action-server action-client; do
        rm -rf "examples/qemu-arm-nuttx/rust/zenoh/$example/target"
    done
    echo "NuttX build artifacts cleaned"
