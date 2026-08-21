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
#   - Run `just setup-nuttx` to download sources
#   - kconfig frontend: a native kconfig-conf, an existing kconfiglib, OR nothing
#     — this script self-provisions kconfiglib into a repo-local venv when neither
#     is present (issue 0431; works on PEP-668 distros where `pip install` is refused).
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
DEFCONFIG="${NUTTX_DEFCONFIG:-$PROJECT_ROOT/packages/boards/nros-board-nuttx-qemu/nuttx-config/arm/defconfig}"

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

# kconfig: NuttX's `make olddefconfig` needs a kconfig frontend. Prefer a native
# `kconfig-conf` or an already-present `kconfiglib` (`olddefconfig` on PATH);
# otherwise SELF-PROVISION kconfiglib into a repo-local venv. Issue 0431 — a host
# that ran only `nros setup <board>` has the toolchain, qemu and sources but NOT
# kconfig, so every NuttX test cell silently skipped. `pip install kconfiglib` (the
# old remedy) is refused on PEP-668 distros (Arch, Debian 12+); a venv's own pip is
# not, so the venv makes the provisioning work everywhere without sudo.
if ! command -v kconfig-conf &>/dev/null && ! command -v olddefconfig &>/dev/null; then
    KCONFIG_VENV="$PROJECT_ROOT/build/nuttx-kconfig-venv"
    if [ ! -x "$KCONFIG_VENV/bin/olddefconfig" ]; then
        echo "kconfig frontend not found — provisioning kconfiglib into $KCONFIG_VENV …"
        if ! command -v python3 &>/dev/null; then
            echo "ERROR: no kconfig-conf/olddefconfig and no python3 to provision kconfiglib." >&2
            echo "  Install a distro package (e.g. kconfig-frontends-nox) or python3." >&2
            exit 1
        fi
        if ! python3 -m venv "$KCONFIG_VENV" 2>/dev/null; then
            echo "ERROR: python3 -m venv failed — install the venv module (e.g. python3-venv)" >&2
            echo "  or a distro kconfig-frontends-nox package." >&2
            rm -rf "$KCONFIG_VENV"
            exit 1
        fi
        if ! "$KCONFIG_VENV/bin/pip" install --quiet kconfiglib; then
            echo "ERROR: could not install kconfiglib into the venv (offline?)." >&2
            echo "  Install a distro kconfig-frontends-nox package instead." >&2
            rm -rf "$KCONFIG_VENV"
            exit 1
        fi
    fi
    export PATH="$KCONFIG_VENV/bin:$PATH"
fi
if ! command -v kconfig-conf &>/dev/null && ! command -v olddefconfig &>/dev/null; then
    echo "ERROR: kconfig tools still unavailable after provisioning (kconfig-conf/olddefconfig)." >&2
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
# phase-339 W1 — ARCH-KEYED everything below.
#
# Both architectures build in this one tree, so a single marker and a single
# export dir made whichever built last the owner: the other arch's already-linked
# entries went stale and its cells stopped running (issue 0433). The tree stays
# shared (NuttX builds in-tree; one `.config` at a time), but the OUTPUT each
# consumer links is now per-arch and survives the other arch's build.
#
# Derived from the DEFCONFIG, never from `.config` — at decision time the tree
# may still hold the other architecture, which is exactly the case this fixes.
NUTTX_ARCH=$(grep -E '^CONFIG_ARCH=' "$DEFCONFIG" 2>/dev/null | cut -d'"' -f2)
if [ -z "$NUTTX_ARCH" ]; then
    echo "build-nuttx.sh: $DEFCONFIG declares no CONFIG_ARCH — cannot key the export" >&2
    exit 1
fi
# `risc-v` → `riscv`: the arch name reaches directory names and env vars.
NUTTX_ARCH="${NUTTX_ARCH//-/}"
# The per-arch snapshot consumers link against (phase-339 W2). Its own key file
# records what produced it, so freshness never depends on the shared tree.
# issue 0750 (B) — the snapshot is named for the CONFIG, not the arch.
#
# Two configurations of ONE arch are a real case (`arm` and a future `arm-smp`:
# same CONFIG_ARCH, same e_machine, different kernels), and naming the snapshot
# `-${NUTTX_ARCH}` would land both in one directory. The key file inside would
# still catch it — it is HEAD:sha256(defconfig) — but only as a rebuild thrash
# on every lane switch, and `nuttx_kernel_path_for()`'s `e_machine` check cannot
# tell the two apart at all (that check exists for arm-vs-riscv, issue 0743).
#
# The id is DERIVED from the defconfig's own directory
# (`nuttx-config/<id>/defconfig`), never a hand-maintained list. The existing
# dirs are `arm` and `riscv`, which are exactly the strings the arch produced,
# so this renames nothing today and gives `arm-smp` for free tomorrow.
NUTTX_CONFIG_ID=$(basename "$(dirname "$DEFCONFIG")")
if [ -z "$NUTTX_CONFIG_ID" ] || [ "$NUTTX_CONFIG_ID" = "." ]; then
    echo "build-nuttx.sh: cannot derive a config id from '$DEFCONFIG' — expected .../<id>/defconfig" >&2
    exit 1
fi
NUTTX_SNAPSHOT="nros-nuttx-export-${NUTTX_CONFIG_ID}"
NUTTX_SNAPSHOT_KEY="${NUTTX_SNAPSHOT}/.nros-export-key"

MARKER=".nros-nuttx-build-head-${NUTTX_CONFIG_ID}"
CURRENT_HEAD=$(git -C "$NUTTX_DIR" rev-parse HEAD 2>/dev/null || echo "unknown")
# 194.5: key the marker on the NuttX HEAD *and* this board's defconfig (content
# hash) so a board/config switch — not just a submodule-HEAD change — forces a
# reconfigure. The old HEAD-only marker silently built a stale or *other-board*
# config when the shared NuttX tree was already configured for a different board
# (the single in-tree .config can only hold one board at a time).
DEFCONFIG_HASH=$(sha256sum "$DEFCONFIG" 2>/dev/null | cut -d' ' -f1)
CURRENT_KEY="${CURRENT_HEAD}:${DEFCONFIG_HASH}"
STORED_KEY=$(cat "$MARKER" 2>/dev/null || echo "none")

# phase-339 W1 — SNAPSHOT SHORT-CIRCUIT, deliberately ahead of every tree check.
#
# This arch's snapshot is valid or not on its own terms: it records the
# `HEAD:defconfig-hash` that produced it, and consumers link IT rather than the
# shared tree. So when the key matches there is nothing to do — no matter which
# architecture the tree currently holds.
#
# Ordering is the whole point. Behind the `NEEDS_RECONFIG` checks below this
# never fires in the case that matters: those checks compare the DEFCONFIG's
# board against the live `.config`, which is the OTHER arch exactly when we most
# want to skip. That is what made `build-fixtures` reconfigure the tree twice per
# run (issue 0433) — and with the snapshot in place, rebuilding is not just slow
# but pointless.
if [ -f "$NUTTX_SNAPSHOT_KEY" ] \
    && [ "$(cat "$NUTTX_SNAPSHOT_KEY" 2>/dev/null)" = "$CURRENT_KEY" ]; then
    echo "NuttX ${NUTTX_ARCH} export up-to-date ($NUTTX_SNAPSHOT) — skipping build/export."
    # Issue 0525 — say so when the SHARED tree is left holding another arch.
    #
    # Skipping is right (issue 0433: reconfiguring twice per `build-fixtures`
    # run is pointless once the snapshot exists), but it has a side effect the
    # caller cannot see: `$NUTTX_DIR/.config` and the generated
    # `include/nuttx/config.h` still describe whichever arch was configured
    # LAST. Anything deriving a compile input from the tree then gets the wrong
    # memory map and the wrong ABI — that is issue 0511, where the ARM image
    # linked against RISC-V's `CONFIG_FLASH_SIZE=0` and every ROM byte
    # "overflowed".
    #
    # The contract is: this path guarantees the SNAPSHOT, never the tree. Making
    # it audible costs one grep and turns a silent trap into a stated fact.
    _want_board=$(sed -n 's/^CONFIG_ARCH_BOARD=//p' "$DEFCONFIG" 2>/dev/null | head -1)
    _have_board=$(sed -n 's/^CONFIG_ARCH_BOARD=//p' .config 2>/dev/null | head -1)
    if [ -n "$_have_board" ] && [ -n "$_want_board" ] && [ "$_have_board" != "$_want_board" ]; then
        echo "  NOTE: the shared tree stays configured for ${_have_board}, not ${_want_board}."
        echo "        This path guarantees the snapshot, not \$NUTTX_DIR. Build inputs must"
        echo "        resolve headers via nros_build_paths::nuttx_include_root, which reads"
        echo "        ${NUTTX_SNAPSHOT}/include (issue 0525; gated by"
        echo "        check-nuttx-shared-tree-headers)."
    fi
    exit 0
fi
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
    # #165 — regenerate a VALID apps/external staging before olddefconfig. The
    # kernel distclean above does NOT touch the apps tree, so a STALE
    # external/Kconfig survives across reconfigures/arch-switches — e.g. an old
    # per-example staging (pre-212.M-F.12) that `source`s per-example Kconfigs
    # which no longer exist, or apps for the other arch. `make olddefconfig` then
    # hard-fails sourcing a missing Kconfig (observed 2026-07-09 on the arm→riscv
    # switch). stage-external-apps writes the current minimal integration-shell
    # Kconfig, so the staging always matches this script's version. Best-effort
    # (a NuttX tree that is not a nano-ros apps tree just keeps its own external/).
    bash "$PROJECT_ROOT/scripts/nuttx/stage-external-apps.sh" "$NUTTX_APPS_DIR" \
        >/dev/null 2>&1 || true
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
    :
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
# `make export` is not idempotent: it mkdir's `nuttx-export-<ver>/` and fails
# ("File exists") on a leftover. Clear only the UNVERSIONED staging products it
# writes — never `nros-nuttx-export-*`, which is the other architecture's
# snapshot (phase-339 W1; the old `rm -rf nuttx-export-*` wiped it).
rm -rf nuttx-export-*.tar.gz nuttx-export-*/
make export
EXPORT_TAR=$(ls nuttx-export-*.tar.gz 2>/dev/null | head -1)
if [ -n "$EXPORT_TAR" ]; then
    EXPORT_DIR="${EXPORT_TAR%.tar.gz}"
    rm -rf "$EXPORT_DIR"
    tar xzf "$EXPORT_TAR"
    # Move the freshly extracted export into this arch's stable snapshot path.
    # Consumers link `<snapshot>/libs`, so the path must not carry the NuttX
    # version (it would change under them on a submodule bump) nor be shared
    # between architectures (the whole defect).
    rm -rf "$NUTTX_SNAPSHOT"
    mv "$EXPORT_DIR" "$NUTTX_SNAPSHOT"
    rm -f "$EXPORT_TAR"

    # The vector table is an intermediate object `make export` does not ship,
    # and the arm image link needs it (riscv sets NUTTX_VECTORTAB="" and skips).
    # Snapshot it beside the other startup objects so nothing reaches back into
    # the live tree.
    for _vt in arch/*/src/*_vectortab.o; do
        if [ -f "$_vt" ]; then
            mkdir -p "$NUTTX_SNAPSHOT/startup"
            cp "$_vt" "$NUTTX_SNAPSHOT/startup/"
        fi
    done

    # Record what produced this snapshot. The short-circuit above reads it, so
    # freshness is a property of the SNAPSHOT rather than of the shared tree.
    echo "$CURRENT_KEY" > "$NUTTX_SNAPSHOT_KEY"
    echo "  Export: $NUTTX_DIR/$NUTTX_SNAPSHOT (arch ${NUTTX_ARCH})"
else
    echo "  WARNING: make export did not produce a tarball"
fi

echo ""
echo "=== Build Complete ==="
echo "  NuttX ELF: $NUTTX_DIR/nuttx"
echo ""
# 194.3c — arch-aware run hint (was hardcoded arm). Derive the qemu machine from
# the configured board arch so a riscv (rv-virt) export prints the right command.
_BUILT_ARCH=$(grep -E '^CONFIG_ARCH=' .config 2>/dev/null | cut -d'"' -f2)
echo "Run with QEMU:"
case "$_BUILT_ARCH" in
    risc-v)
        echo "  qemu-system-riscv32 -M virt -bios none -nographic \\"
        echo "      -kernel $NUTTX_DIR/nuttx \\"
        echo "      -netdev user,id=u1 -device virtio-net-device,netdev=u1"
        ;;
    *)
        echo "  qemu-system-arm -M virt -cpu cortex-a7 -nographic \\"
        echo "      -kernel $NUTTX_DIR/nuttx \\"
        echo "      -nic tap,ifname=tap-qemu0,script=no,downscript=no"
        ;;
esac
