set working-directory := '..'

NIGHTLY := `awk '/^channel/ {gsub(/"/, "", $3); print $3; exit}' tools/rust-toolchain.toml`

# Phase 100.7 — AGX Orin SPE (Cortex-R5F, NVIDIA FreeRTOS V10.4.3 FSP).
#
# The board crate is **excluded from the workspace** because the rest of
# the workspace can't see the NVIDIA FSP. These recipes drive it out-of-
# tree against an env-pointed FSP install (`NV_SPE_FSP_DIR`).
#
# The POSIX-side mock-IVC test (Phase 100.8) exercises the framing
# protocol via the `nvidia-ivc/unix-mock` backend and lives in
# `nros-tests` — `just orin_spe test` dispatches there so the per-
# platform CI surface matches the other modules.

default:
    @just --list --justfile {{source_file()}}

# =============================================================================
# Setup / doctor
# =============================================================================

# Install Orin SPE prerequisites — the `armv7r-none-eabihf` target on
# the workspace nightly + a verbose "you also need NV_SPE_FSP_DIR"
# notice. The FSP itself ships under NVIDIA's SDK Manager EULA and
# cannot be auto-installed.
setup:
    #!/usr/bin/env bash
    set -e
    echo "Installing armv7r-none-eabihf rustup target on {{NIGHTLY}}..."
    rustup target add armv7r-none-eabihf --toolchain {{NIGHTLY}} >/dev/null
    echo "  [OK] armv7r-none-eabihf installed"
    if [ -z "${NV_SPE_FSP_DIR:-}" ]; then
        echo ""
        echo "  [INFO] NV_SPE_FSP_DIR not set."
        echo "         To build the SPE board crate ('just orin_spe build'), install"
        echo "         NVIDIA's SPE FSP via SDK Manager and export:"
        echo "             export NV_SPE_FSP_DIR=\$HOME/nvidia/spe-fsp"
        echo "         (must contain lib/libtegra_aon_fsp.a + include/)"
    fi
    if ! command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        echo "  [INFO] arm-none-eabi-gcc not on PATH — needed by zpico-sys's cc::Build."
        echo "         apt: sudo apt install gcc-arm-none-eabi"
    fi

# Diagnose Orin SPE install status (read-only).
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    if rustup target list --installed --toolchain {{NIGHTLY}} 2>/dev/null \
            | grep -qx 'armv7r-none-eabihf'; then
        echo "  [OK] armv7r-none-eabihf target ({{NIGHTLY}})"
    else
        echo "  [MISSING] armv7r-none-eabihf target on {{NIGHTLY}} (run: just orin_spe setup)"
        fail=1
    fi
    if command -v arm-none-eabi-gcc >/dev/null 2>&1; then
        echo "  [OK] arm-none-eabi-gcc"
    else
        echo "  [MISSING] arm-none-eabi-gcc (run: just workspace apt-packages)"
        fail=1
    fi
    if [ -n "${NV_SPE_FSP_DIR:-}" ] && [ -d "$NV_SPE_FSP_DIR/lib" ]; then
        echo "  [OK] NV_SPE_FSP_DIR=$NV_SPE_FSP_DIR"
    elif [ -n "${NV_SPE_FSP_DIR:-}" ]; then
        echo "  [WARN] NV_SPE_FSP_DIR=$NV_SPE_FSP_DIR but no lib/ subdir"
        # Don't fail — host CI without the SDK is the common case.
    else
        echo "  [INFO] NV_SPE_FSP_DIR not set (hardware build needs it; POSIX path doesn't)"
    fi
    exit $fail

# =============================================================================
# Build
# =============================================================================

# Build the SPE board crate against an env-pointed NVIDIA FSP install.
# Produces an rlib at:
#   packages/boards/nros-board-orin-spe/target/armv7r-none-eabihf/release/
# The firmware crate (downstream — autoware_sentinel side) wraps this
# rlib into a staticlib and links via NVIDIA's Makefile.
build:
    #!/usr/bin/env bash
    set -e
    if [ -z "${NV_SPE_FSP_DIR:-}" ]; then
        echo "ERROR: NV_SPE_FSP_DIR is not set."
        echo "       Install NVIDIA's SPE FSP via SDK Manager and export the path."
        echo "       For the POSIX-only mock-IVC path, use 'just orin_spe test' instead."
        exit 1
    fi
    cd packages/boards/nros-board-orin-spe
    cargo +{{NIGHTLY}} build --release

# Build POSIX-side fixtures used by the mock-IVC test (Phase 100.8).
# These exercise the unix-mock backend and don't need NV_SPE_FSP_DIR.
build-fixtures:
    #!/usr/bin/env bash
    set -e
    echo "Building Orin SPE POSIX fixtures..."
    cargo build -p nvidia-ivc --features unix-mock --quiet
    if [ -d examples/orin-spe/rust/zenoh/talker ]; then
        echo "  → orin-spe/rust/zenoh/talker"
        ( cd examples/orin-spe/rust/zenoh/talker && cargo build --release --quiet )
    fi

# Run Orin SPE tests — POSIX mock-IVC E2E in nros-tests, plus the
# nvidia-ivc unix-mock loopback. Real-hardware tests live in
# autoware_sentinel Phase 11.7.
test verbose="":
    #!/usr/bin/env bash
    set -e
    echo "=== nvidia-ivc unix-mock loopback ==="
    cargo test -p nvidia-ivc --features unix-mock
    echo ""
    echo "=== orin-spe POSIX mock-IVC E2E ==="
    just _nextest-platform orin_spe_mock_ivc '{{verbose}}'

# Flash the SPE firmware partition (A_spe-fw) from an x86 host with
# the Orin in USB recovery mode. Requires the L4T BSP at
# $L4T_BSP_DIR (typically ~/nvidia/Linux_for_Tegra) and root for
# flash.sh's USB access. QSPI hardware firewall blocks userland dd
# from Linux, so host USB recovery is the only single-partition path.
flash:
    #!/usr/bin/env bash
    set -e
    if [ -z "${L4T_BSP_DIR:-}" ]; then
        echo "ERROR: L4T_BSP_DIR is not set."
        echo "       Export the L4T BSP root (e.g. ~/nvidia/Linux_for_Tegra)."
        exit 1
    fi
    if [ ! -x "$L4T_BSP_DIR/flash.sh" ]; then
        echo "ERROR: $L4T_BSP_DIR/flash.sh not found or not executable."
        exit 1
    fi
    echo "Put the AGX Orin DevKit in USB recovery mode now."
    echo "Press Enter to continue, Ctrl-C to abort..."
    read -r
    cd "$L4T_BSP_DIR"
    sudo ./flash.sh -k A_spe-fw jetson-agx-orin-devkit mmcblk0p1

# CI: doctor + test (POSIX path only — hardware tests are downstream).
ci: doctor test

# Clean Orin SPE build artifacts.
clean:
    #!/usr/bin/env bash
    rm -rf packages/boards/nros-board-orin-spe/target
    if [ -d examples/orin-spe ]; then
        find examples/orin-spe -type d -name target -exec rm -rf {} + 2>/dev/null || true
    fi
    echo "Orin SPE build artifacts cleaned"
