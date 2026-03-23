set working-directory := '..'

# Pinned NuttX version
NUTTX_TAG := "nuttx-12.8.0"

# Install NuttX ARM libraries into build/install/
install:
    #!/usr/bin/env bash
    set -e
    NUTTX_EXPORT=$(ls -d "external/nuttx/nuttx-export-"* 2>/dev/null | head -1)
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

# Download NuttX RTOS and apps sources
setup:
    #!/usr/bin/env bash
    set -e
    echo "=== NuttX Setup ==="
    echo ""
    NUTTX_DIR="$(pwd)/external/nuttx"
    NUTTX_APPS_DIR="$(pwd)/external/nuttx-apps"

    # --- NuttX RTOS ---
    if [ -d "$NUTTX_DIR/include" ]; then
        tag=$(cd "$NUTTX_DIR" && git describe --tags --exact-match 2>/dev/null || echo "unknown")
        echo "NuttX already present at $NUTTX_DIR (tag: $tag)"
        if [ "$tag" != "{{NUTTX_TAG}}" ]; then
            echo "  WARNING: expected {{NUTTX_TAG}}, found $tag"
            echo "  To update: rm -rf $NUTTX_DIR && just nuttx setup"
        fi
    else
        echo "Cloning NuttX {{NUTTX_TAG}}..."
        git clone --depth 1 --branch "{{NUTTX_TAG}}" \
            https://github.com/apache/nuttx.git "$NUTTX_DIR"
        echo "  -> $NUTTX_DIR"
    fi
    echo ""

    # --- NuttX Apps ---
    if [ -d "$NUTTX_APPS_DIR/include" ]; then
        tag=$(cd "$NUTTX_APPS_DIR" && git describe --tags --exact-match 2>/dev/null || echo "unknown")
        echo "NuttX apps already present at $NUTTX_APPS_DIR (tag: $tag)"
        if [ "$tag" != "{{NUTTX_TAG}}" ]; then
            echo "  WARNING: expected {{NUTTX_TAG}}, found $tag"
            echo "  To update: rm -rf $NUTTX_APPS_DIR && just nuttx setup"
        fi
    else
        echo "Cloning NuttX apps {{NUTTX_TAG}}..."
        git clone --depth 1 --branch "{{NUTTX_TAG}}" \
            https://github.com/apache/nuttx-apps.git "$NUTTX_APPS_DIR"
        echo "  -> $NUTTX_APPS_DIR"
    fi
    echo ""

    # --- Configure and build NuttX kernel ---
    BOARD_DIR="$(pwd)/packages/boards/nros-nuttx-qemu-arm"
    HAS_CROSS=false; command -v arm-none-eabi-gcc &>/dev/null && HAS_CROSS=true
    HAS_KCONFIG=false; (command -v kconfig-conf &>/dev/null || command -v olddefconfig &>/dev/null) && HAS_KCONFIG=true
    if $HAS_CROSS && $HAS_KCONFIG; then
        echo "Configuring and building NuttX kernel..."
        NUTTX_DIR="$NUTTX_DIR" NUTTX_APPS_DIR="$NUTTX_APPS_DIR" \
            "$BOARD_DIR/scripts/build-nuttx.sh"
    else
        echo "WARNING: Skipping NuttX kernel build (missing dependencies)."
        $HAS_CROSS   || echo "  - arm-none-eabi-gcc not found (sudo apt install gcc-arm-none-eabi)"
        $HAS_KCONFIG || echo "  - kconfig tools not found (sudo apt install kconfig-frontends-nox)"
        echo "  Install and run: just nuttx build-kernel"
    fi
    echo ""

    echo "=== Environment Variables ==="
    echo ""
    echo "Add these to your shell or .cargo/config.toml [env] section:"
    echo ""
    echo "  export NUTTX_DIR=$NUTTX_DIR"
    echo "  export NUTTX_APPS_DIR=$NUTTX_APPS_DIR"
    echo ""

    # --- Patched libc for NuttX Rust examples ---
    LIBC_DIR="$(pwd)/external/libc"
    LIBC_VERSION="0.2.178"
    NUTTX_MOD=""
    for candidate in ~/.cargo/registry/src/*/libc-${LIBC_VERSION}; do
        if [ -f "$candidate/Cargo.toml" ]; then
            NUTTX_MOD="$candidate/src/unix/nuttx/mod.rs"
            CACHED_LIBC="$candidate"
            break
        fi
    done
    if [ -z "$NUTTX_MOD" ]; then
        echo "WARNING: libc ${LIBC_VERSION} not found in Cargo cache."
        echo "  Run: cargo fetch (in any workspace directory) to populate the cache."
        echo "  Then re-run: just nuttx setup"
    elif [ -d "$LIBC_DIR" ] && grep -q '_SC_HOST_NAME_MAX' "$LIBC_DIR/src/unix/nuttx/mod.rs" 2>/dev/null; then
        echo "Patched libc already present at $LIBC_DIR"
    else
        echo "Setting up patched libc ${LIBC_VERSION} for NuttX..."
        rm -rf "$LIBC_DIR"
        cp -r "$CACHED_LIBC" "$LIBC_DIR"
        patch -p1 -d "$LIBC_DIR" \
            < "$(pwd)/packages/boards/nros-nuttx-qemu-arm/patches/libc-nuttx-sc-host-name-max.patch"
        echo "  -> $LIBC_DIR"
    fi
    echo ""

    echo "Setup complete!"

# Clean NuttX build artifacts
clean:
    #!/usr/bin/env bash
    rm -rf build/cmake-nuttx-armv7a-zenoh
    for example in talker listener service-server service-client action-server action-client; do
        rm -rf "examples/qemu-arm-nuttx/rust/zenoh/$example/target"
    done
    echo "NuttX build artifacts cleaned"
