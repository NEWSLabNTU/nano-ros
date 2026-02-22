#!/usr/bin/env bash
#
# Build NuttX QEMU ARM virt image with an nros Rust application.
#
# This script:
#   1. Configures NuttX with the nros defconfig (networking + POSIX + virtio-net)
#   2. Builds the NuttX kernel + apps
#   3. Outputs a bootable ELF at $NUTTX_DIR/nuttx
#
# Prerequisites:
#   - NUTTX_DIR set to NuttX source (e.g., external/nuttx)
#   - NUTTX_APPS_DIR set to NuttX apps source (e.g., external/nuttx-apps)
#   - ARM cross-compiler: arm-none-eabi-gcc
#   - Run `just setup-nuttx` to download sources
#
# Usage:
#   ./build-nuttx.sh                    # Build with default defconfig
#   ./build-nuttx.sh --clean            # Clean build artifacts
#   ./build-nuttx.sh --menuconfig       # Run NuttX menuconfig
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BOARD_DIR="$(dirname "$SCRIPT_DIR")"
DEFCONFIG="$BOARD_DIR/nuttx-config/defconfig"

# --- Validate environment ---

if [ -z "${NUTTX_DIR:-}" ]; then
    echo "ERROR: NUTTX_DIR not set."
    echo "Run: just setup-nuttx"
    echo "Then: export NUTTX_DIR=\$PWD/external/nuttx"
    exit 1
fi

if [ ! -d "$NUTTX_DIR/include" ]; then
    echo "ERROR: NUTTX_DIR ($NUTTX_DIR) does not contain include/"
    echo "Run: just setup-nuttx"
    exit 1
fi

NUTTX_APPS_DIR="${NUTTX_APPS_DIR:-$(dirname "$NUTTX_DIR")/nuttx-apps}"
if [ ! -d "$NUTTX_APPS_DIR" ]; then
    echo "ERROR: NuttX apps not found at $NUTTX_APPS_DIR"
    echo "Run: just setup-nuttx"
    exit 1
fi

if ! command -v arm-none-eabi-gcc &>/dev/null; then
    echo "ERROR: arm-none-eabi-gcc not found."
    echo "Install: sudo apt install gcc-arm-none-eabi"
    exit 1
fi

# --- Handle arguments ---

case "${1:-}" in
    --clean)
        echo "Cleaning NuttX build..."
        cd "$NUTTX_DIR"
        make distclean 2>/dev/null || true
        echo "Done."
        exit 0
        ;;
    --menuconfig)
        echo "Running NuttX menuconfig..."
        cd "$NUTTX_DIR"
        if [ ! -f .config ]; then
            cp "$DEFCONFIG" .config
            make olddefconfig
        fi
        make menuconfig
        echo "Save defconfig with: make savedefconfig"
        exit 0
        ;;
esac

# --- Configure NuttX ---

echo "=== NuttX Build ==="
echo "  NUTTX_DIR:      $NUTTX_DIR"
echo "  NUTTX_APPS_DIR: $NUTTX_APPS_DIR"
echo "  DEFCONFIG:      $DEFCONFIG"
echo ""

cd "$NUTTX_DIR"

# Set apps directory for NuttX build system
export APPDIR="$NUTTX_APPS_DIR"

# Copy defconfig and resolve defaults
if [ ! -f .config ] || [ "$DEFCONFIG" -nt .config ]; then
    echo "Configuring NuttX..."
    cp "$DEFCONFIG" .config
    make olddefconfig
fi

# --- Build NuttX ---

echo "Building NuttX..."
NCPUS=$(nproc 2>/dev/null || echo 4)
make -j"$NCPUS"

echo ""
echo "=== Build Complete ==="
echo "  NuttX ELF: $NUTTX_DIR/nuttx"
echo ""
echo "Run with QEMU:"
echo "  qemu-system-arm -M virt -cpu cortex-a7 -nographic \\"
echo "      -kernel $NUTTX_DIR/nuttx \\"
echo "      -nic tap,ifname=tap-qemu0,script=no,downscript=no"
