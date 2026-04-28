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
#   - ARM cross-compiler: arm-none-eabi-gcc
#   - kconfig-frontends-nox (sudo apt install kconfig-frontends-nox) or Python kconfiglib
#   - Run `just setup-nuttx` to download sources
#
# Environment (auto-resolved from project root if not set):
#   - NUTTX_DIR — NuttX source (default: third-party/nuttx/nuttx)
#   - NUTTX_APPS_DIR — NuttX apps source (default: third-party/nuttx/nuttx-apps)
#
# Usage:
#   ./build-nuttx.sh                    # Build with default defconfig
#   ./build-nuttx.sh --clean            # Clean build artifacts
#   ./build-nuttx.sh --menuconfig       # Run NuttX menuconfig
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
BOARD_DIR="$(dirname "$SCRIPT_DIR")"
PROJECT_ROOT="$(cd "$BOARD_DIR/../../../.." && pwd)"
DEFCONFIG="$BOARD_DIR/nuttx-config/defconfig"

# --- Auto-resolve paths from project root if not set ---

NUTTX_DIR="${NUTTX_DIR:-$PROJECT_ROOT/third-party/nuttx/nuttx}"
NUTTX_APPS_DIR="${NUTTX_APPS_DIR:-$PROJECT_ROOT/third-party/nuttx/nuttx-apps}"

# --- Validate environment ---

if [ ! -d "$NUTTX_DIR" ]; then
    echo "ERROR: NuttX not found at $NUTTX_DIR."
    echo "Run: just setup-nuttx"
    exit 1
fi

if [ ! -d "$NUTTX_DIR/include" ]; then
    echo "ERROR: NUTTX_DIR ($NUTTX_DIR) does not contain include/"
    echo "Run: just setup-nuttx"
    exit 1
fi

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

if ! command -v kconfig-conf &>/dev/null && ! command -v olddefconfig &>/dev/null; then
    echo "ERROR: kconfig tools not found (kconfig-conf or kconfiglib)."
    echo "Install one of:"
    echo "  sudo apt install kconfig-frontends-nox  # Native C implementation (recommended)"
    echo "  pip install kconfiglib                  # Python implementation"
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

# Configure NuttX: symlink Make.defs from the board, copy our defconfig, resolve.
# This replicates what tools/configure.sh does without requiring the full script
# (which has additional dependencies like kconfig-tweak for host detection).
#
# Also detect a stale build tree: .depend files generated against a previous
# NuttX checkout can reference files that have since been moved (e.g.
# stdio/lib_libbsprintf.c → stream/lib_libbsprintf.c after an upstream
# reorganization), causing "No rule to make target" failures. We track the
# NuttX submodule HEAD in a marker file and distclean when it changes.
BOARD_MAKEDEFS="$(pwd)/boards/arm/qemu/qemu-armv7a/scripts/Make.defs"
MARKER=".nros-nuttx-build-head"
CURRENT_HEAD=$(git -C "$NUTTX_DIR" rev-parse HEAD 2>/dev/null || echo "unknown")
STORED_HEAD=$(cat "$MARKER" 2>/dev/null || echo "none")
NEEDS_RECONFIG=0

if [ ! -f .config ] || [ ! -f Make.defs ] || [ "$DEFCONFIG" -nt .config ]; then
    NEEDS_RECONFIG=1
fi
if [ "$CURRENT_HEAD" != "$STORED_HEAD" ]; then
    echo "NuttX submodule HEAD changed ($STORED_HEAD → $CURRENT_HEAD) — cleaning stale build artifacts."
    NEEDS_RECONFIG=1
fi

if [ "$NEEDS_RECONFIG" -eq 1 ]; then
    echo "Configuring NuttX..."
    make distclean 2>/dev/null || true
    rm -f .config Make.defs
    ln -sf "$BOARD_MAKEDEFS" Make.defs
    cp "$DEFCONFIG" .config
    make olddefconfig
    echo "$CURRENT_HEAD" > "$MARKER"
fi

# --- Build NuttX ---

echo "Building NuttX..."
NCPUS=$(nproc 2>/dev/null || echo 4)
make -j"$NCPUS"

# --- Export NuttX for external C/C++ apps ---

echo "Exporting NuttX..."
make export
EXPORT_TAR=$(ls nuttx-export-*.tar.gz 2>/dev/null | head -1)
if [ -n "$EXPORT_TAR" ]; then
    EXPORT_DIR="${EXPORT_TAR%.tar.gz}"
    rm -rf "$EXPORT_DIR"
    tar xzf "$EXPORT_TAR"
    echo "  Export: $NUTTX_DIR/$EXPORT_DIR"
else
    echo "  WARNING: make export did not produce a tarball"
fi

echo ""
echo "=== Build Complete ==="
echo "  NuttX ELF: $NUTTX_DIR/nuttx"
echo ""
echo "Run with QEMU:"
echo "  qemu-system-arm -M virt -cpu cortex-a7 -nographic \\"
echo "      -kernel $NUTTX_DIR/nuttx \\"
echo "      -nic tap,ifname=tap-qemu0,script=no,downscript=no"
