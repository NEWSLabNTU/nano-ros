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
#   - NUTTX_DEFCONFIG — board defconfig (the board overlay supplies this; default
#     is the qemu-arm board)
#   - NUTTX_BOARD_MAKEDEFS — board Make.defs path relative to NUTTX_DIR (the board
#     overlay supplies this; default = the qemu-arm board's
#     boards/arm/qemu/qemu-armv7a/scripts/Make.defs)
#
# Usage:
#   ./build-nuttx.sh                    # Build with default defconfig
#   ./build-nuttx.sh --clean            # Clean build artifacts
#   ./build-nuttx.sh --menuconfig       # Run NuttX menuconfig
#
set -euo pipefail

# This script lives in the shared build-script dir (scripts/nuttx/) so the NuttX
# builders are self-contained — the board-specific input (the defconfig) is
# supplied by the board overlay via NUTTX_DEFCONFIG, not derived from the script's
# location. PROJECT_ROOT resolves two levels up (scripts/nuttx → repo root).
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
DEFCONFIG="${NUTTX_DEFCONFIG:-$PROJECT_ROOT/packages/boards/nros-board-nuttx-qemu-arm/nuttx-config/defconfig}"

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

# 194.1: the cross-compiler is per-board (the board overlay / env sets
# NUTTX_CROSS; arm-none-eabi-gcc is the default for the qemu-arm board). NuttX's
# `make` selects the actual toolchain from the defconfig's CONFIG_ARCH_TOOLCHAIN
# + PATH; this is just a presence check with a board-correct hint.
NUTTX_CROSS="${NUTTX_CROSS:-arm-none-eabi-gcc}"
if ! command -v "$NUTTX_CROSS" &>/dev/null; then
    echo "ERROR: NuttX cross-compiler '$NUTTX_CROSS' not found on PATH."
    echo "Install it (e.g. \`nros setup <board>\` / \`sudo apt install gcc-arm-none-eabi\`)"
    echo "or set NUTTX_CROSS to your board's toolchain."
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

# 194.4: serialize concurrent provisioning. Under the CMake self-provision path
# (`nros_nuttx_build_example`), many parallel example builds invoke this script
# against the *single shared* in-tree NuttX; without a lock their `make` /
# `make export` race (duplicate export dir, `.version.tmp` clobber). The lock +
# the up-to-date short-circuit below make all-but-the-first invocation a no-op.
exec 9>".nros-nuttx-build.lock"
flock 9

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
# 194.3c.3: the board Make.defs path is per-board (arch/chip/board), supplied by
# the board overlay via NUTTX_BOARD_MAKEDEFS (relative to NUTTX_DIR); default =
# the qemu-arm board so the arm provisioning is unchanged. A new-arch board
# (e.g. riscv rv-virt) sets NUTTX_BOARD_MAKEDEFS=boards/risc-v/qemu-rv/rv-virt/scripts/Make.defs.
BOARD_MAKEDEFS="$(pwd)/${NUTTX_BOARD_MAKEDEFS:-boards/arm/qemu/qemu-armv7a/scripts/Make.defs}"
MARKER=".nros-nuttx-build-head"
CURRENT_HEAD=$(git -C "$NUTTX_DIR" rev-parse HEAD 2>/dev/null || echo "unknown")
# 194.5: key the marker on the NuttX HEAD *and* this board's defconfig (content
# hash) so a board/config switch — not just a submodule-HEAD change — forces a
# reconfigure. The old HEAD-only marker silently built a stale or *other-board*
# config when the shared NuttX tree was already configured for a different board
# (the single in-tree .config can only hold one board at a time).
DEFCONFIG_HASH=$(sha256sum "$DEFCONFIG" 2>/dev/null | cut -d' ' -f1)
CURRENT_KEY="${CURRENT_HEAD}:${DEFCONFIG_HASH}"
STORED_KEY=$(cat "$MARKER" 2>/dev/null || echo "none")
# Self-validate the in-tree config against this board (catches an external
# reconfigure that didn't touch the marker).
EXPECTED_BOARD=$(grep -E '^CONFIG_ARCH_BOARD=' "$DEFCONFIG" 2>/dev/null || true)
ACTUAL_BOARD=$(grep -E '^CONFIG_ARCH_BOARD=' .config 2>/dev/null || true)
NEEDS_RECONFIG=0

if [ ! -f .config ] || [ ! -f Make.defs ]; then
    NEEDS_RECONFIG=1
fi
if [ "$CURRENT_KEY" != "$STORED_KEY" ]; then
    echo "NuttX HEAD/defconfig changed ($STORED_KEY → $CURRENT_KEY) — reconfiguring."
    NEEDS_RECONFIG=1
fi
if [ -n "$EXPECTED_BOARD" ] && [ "$EXPECTED_BOARD" != "$ACTUAL_BOARD" ]; then
    echo "NuttX tree is configured for '${ACTUAL_BOARD:-<none>}', need '$EXPECTED_BOARD' — reconfiguring."
    NEEDS_RECONFIG=1
fi

if [ "$NEEDS_RECONFIG" -eq 1 ]; then
    echo "Configuring NuttX..."
    make distclean 2>/dev/null || true
    rm -f .config Make.defs
    ln -sf "$BOARD_MAKEDEFS" Make.defs
    cp "$DEFCONFIG" .config
    make olddefconfig
    echo "$CURRENT_KEY" > "$MARKER"
fi

# 194.4: true up-to-date short-circuit. When no reconfigure was needed (HEAD +
# defconfig + board all match the marker) AND a completed export is present,
# the export is already current — skip `make`/`make export` entirely so the
# provision is a real no-op (build-once-link-many). The export-presence check
# also recovers from a prior run that reconfigured but failed mid-build (fresh
# marker, missing export ⇒ NEEDS_RECONFIG=0 but no tarball ⇒ rebuild).
if [ "$NEEDS_RECONFIG" -eq 0 ]; then
    # `|| true`: under `set -o pipefail`, `ls <glob>` returns nonzero when no
    # tarball matches, which would abort the script (set -e) before we can
    # decide to rebuild. Tolerate the empty match.
    _EXPORT_TAR=$(ls nuttx-export-*.tar.gz 2>/dev/null | head -1 || true)
    if [ -n "$_EXPORT_TAR" ] && [ -d "${_EXPORT_TAR%.tar.gz}" ]; then
        echo "NuttX export up-to-date ($_EXPORT_TAR) — skipping build/export."
        exit 0
    fi
fi

# --- Build NuttX ---

echo "Building NuttX..."
NCPUS=$(nproc 2>/dev/null || echo 4)
make -j"$NCPUS"

# --- Export NuttX for external C/C++ apps ---

# `make export` is not idempotent: it mkdir's `nuttx-export-<ver>/` and fails
# ("File exists") if a prior run left that dir (or a stale tarball) behind.
# Clear both so export always starts clean (194.4 — repeated cmake-driven
# provisioning would otherwise wedge on the leftover from an interrupted run).
echo "Exporting NuttX..."
rm -rf nuttx-export-*.tar.gz nuttx-export-*/
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
