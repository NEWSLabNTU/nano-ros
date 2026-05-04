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
    elif [ -f "external/spe-fsp/install/lib/libtegra_aon_fsp.a" ]; then
        echo "  [OK] FSP staged at external/spe-fsp/install (run 'just orin_spe bsp-build' to refresh)"
        echo "       export NV_SPE_FSP_DIR=$(pwd)/external/spe-fsp/install"
    else
        echo "  [INFO] NV_SPE_FSP_DIR not set and no staged FSP under external/."
        echo "         For hardware builds: just orin_spe bsp-download && just orin_spe bsp-build"
        echo "         For POSIX-only path: just orin_spe test"
    fi
    exit $fail

# =============================================================================
# BSP download + library build
# =============================================================================
#
# The SPE FreeRTOS BSP is an NVIDIA-hosted source tarball under L4T's
# `public_sources.tbz2`. These recipes idempotently fetch it, build the
# upstream demo (`rt-aux-cpu-demo-fsp/Makefile`), and repackage the
# resulting `.o` files into a single `libtegra_aon_fsp.a` archive plus
# a staged `include/` tree that match the layout `nros-board-orin-spe`'s
# `build.rs` expects under `NV_SPE_FSP_DIR`.
#
# Env knobs:
#   SPE_BSP_DOWNLOAD_DIR  — where to keep tarballs + extracted sources
#                           (default: external/spe-fsp/downloads)
#   SPE_BSP_SRC_DIR       — pre-extracted BSP root to reuse
#                           (default: $SPE_BSP_DOWNLOAD_DIR/spe-freertos-bsp)
#   SPE_FSP_PREFIX        — staged install prefix
#                           (default: external/spe-fsp/install)
#   ARM_TOOLCHAIN_DIR     — pre-extracted arm-none-eabi toolchain root
#                           (default: $SPE_BSP_DOWNLOAD_DIR/arm-gnu-toolchain-13.2.rel1)

# Idempotent download of the BSP + ARM GNU toolchain (cksum-verified,
# skips when artifacts already exist). If `SPE_BSP_SRC_DIR` points at
# an already-extracted tree (e.g. autoware_sentinel's
# `scripts/spe/downloads/spe-freertos-bsp/`), this recipe reuses it
# instead of re-downloading.
bsp-download:
    #!/usr/bin/env bash
    set -euo pipefail
    DL="${SPE_BSP_DOWNLOAD_DIR:-external/spe-fsp/downloads}"
    SRC="${SPE_BSP_SRC_DIR:-$DL/spe-freertos-bsp}"
    TC="${ARM_TOOLCHAIN_DIR:-$DL/arm-gnu-toolchain-13.2.rel1}"

    PUB_URL="https://developer.nvidia.com/downloads/embedded/l4t/r36_release_v4.4/sources/public_sources.tbz2"
    PUB_CK="3391786610"
    TC_URL="https://developer.arm.com/-/media/Files/downloads/gnu/13.2.rel1/binrel/arm-gnu-toolchain-13.2.rel1-x86_64-arm-none-eabi.tar.xz"
    TC_CK="1842230340"

    info(){ printf '\033[1;34m==> %s\033[0m\n' "$*"; }

    check_cksum(){ [ -f "$1" ] && [ -n "$2" ] && \
        [ "$(cksum "$1" | awk '{print $1}')" = "$2" ]; }

    download_if_needed(){
        local url="$1" dest="$2" expected="$3"
        if check_cksum "$dest" "$expected"; then
            info "Checksum OK, skipping: $(basename "$dest")"; return 0; fi
        info "Downloading $(basename "$dest")"
        mkdir -p "$(dirname "$dest")"
        curl -L --fail --progress-bar -o "$dest.part" "$url"
        mv "$dest.part" "$dest"
    }

    if [ -d "$SRC/fsp/source" ]; then
        info "Reusing pre-extracted BSP at $SRC"
    else
        mkdir -p "$DL"
        PUB="$DL/public_sources.tbz2"
        download_if_needed "$PUB_URL" "$PUB" "$PUB_CK"
        info "Locating SPE tarball inside public_sources.tbz2"
        SPE_INNER=$(tar -tjf "$PUB" | grep -iE 'spe|freertos|aux.cpu' | head -1)
        [ -n "$SPE_INNER" ] || { echo "no SPE tarball found"; exit 1; }
        info "Found: $SPE_INNER"
        tar -xjf "$PUB" -C "$DL" "$SPE_INNER"
        mkdir -p "$SRC"
        tar -xf "$DL/$SPE_INNER" -C "$SRC" --strip-components=1 2>/dev/null \
            || tar -xf "$DL/$SPE_INNER" -C "$SRC"
        info "BSP extracted to $SRC"
    fi

    if [ -d "$TC/bin" ]; then
        info "Reusing toolchain at $TC"
    else
        TC_AR="$DL/arm-gnu-toolchain-13.2.rel1-x86_64-arm-none-eabi.tar.xz"
        download_if_needed "$TC_URL" "$TC_AR" "$TC_CK"
        info "Extracting toolchain to $TC"
        mkdir -p "$TC"
        tar -xJf "$TC_AR" -C "$TC" --strip-components=1
    fi

    info "BSP ready:"
    echo "  source:    $SRC"
    echo "  toolchain: $TC"

# Build the FSP + FreeRTOS sources via the upstream
# `rt-aux-cpu-demo-fsp/Makefile`, then repackage all object files
# (excluding the demo's `main.o` / `app_init.o` entry points so they
# don't collide with nano-ros's `nros_app_rust_entry`) into
# `libtegra_aon_fsp.a`. Stages headers + archive into `SPE_FSP_PREFIX`.
#
# The upstream Makefile only ever builds an ELF; we re-use its `.o`
# outputs because reconstructing its include / preprocessor flag set
# from scratch would be brittle across NVIDIA SDK Manager versions.
bsp-build:
    #!/usr/bin/env bash
    set -euo pipefail
    DL="${SPE_BSP_DOWNLOAD_DIR:-external/spe-fsp/downloads}"
    SRC="${SPE_BSP_SRC_DIR:-$DL/spe-freertos-bsp}"
    TC="${ARM_TOOLCHAIN_DIR:-$DL/arm-gnu-toolchain-13.2.rel1}"
    PREFIX="${SPE_FSP_PREFIX:-$(pwd)/external/spe-fsp/install}"

    if [ ! -d "$SRC/fsp/source" ]; then
        echo "ERROR: BSP not found at $SRC. Run: just orin_spe bsp-download"
        exit 1
    fi
    if [ ! -x "$TC/bin/arm-none-eabi-gcc" ]; then
        echo "ERROR: toolchain not found at $TC/bin. Run: just orin_spe bsp-download"
        exit 1
    fi

    info(){ printf '\033[1;34m==> %s\033[0m\n' "$*"; }

    info "Building rt-aux-cpu-demo-fsp (SPE_FREERTOS_BSP=$SRC)"
    # Upstream Makefile expects absolute SPE_FREERTOS_BSP and a
    # CROSS_COMPILE prefix that ends in `-`. The root justfile exports
    # FREERTOS_DIR / FREERTOS_PORT pointing at the MPS2 Cortex-M3
    # kernel; override on the make command line so the upstream
    # `?=` defaults to the BSP's bundled V10.4.3 + ARM_R5 port instead.
    make -C "$SRC/rt-aux-cpu-demo-fsp" -j"$(nproc)" \
        SPE_FREERTOS_BSP="$SRC" \
        FREERTOS_DIR="$SRC/FreeRTOSV10.4.3/FreeRTOS/Source" \
        FREERTOS_PORT="GCC/ARM_R5" \
        FSP_SRC_DIR="$SRC/fsp/source" \
        CROSS_COMPILE="$TC/bin/arm-none-eabi-" \
        bin_t23x

    OUTDIR="$SRC/rt-aux-cpu-demo-fsp/out/t23x"
    if [ ! -d "$OUTDIR" ]; then
        echo "ERROR: upstream build produced no $OUTDIR"
        exit 1
    fi

    info "Packaging FSP/FreeRTOS objects into libtegra_aon_fsp.a"
    mkdir -p "$PREFIX/lib" "$PREFIX/include"

    # Drop demo entry points so the consumer crate's `nros_app_rust_entry`
    # / `app_init` (or NVIDIA's downstream firmware) own those slots.
    OBJS=$(find "$OUTDIR" -name '*.o' \
        -not -name 'main.o' \
        -not -name 'app_init.o' \
        -not -name 'startup.o' \
        | sort)
    if [ -z "$OBJS" ]; then
        echo "ERROR: no .o files under $OUTDIR"; exit 1
    fi
    rm -f "$PREFIX/lib/libtegra_aon_fsp.a"
    "$TC/bin/arm-none-eabi-ar" rcs "$PREFIX/lib/libtegra_aon_fsp.a" $OBJS

    # Newlib stub: nano-ros's board crate emits `-lnewlib` by default
    # (overridable via NV_SPE_FSP_LIBC). Provide a thin alias archive
    # that re-exports the toolchain's libc so naive `-lnewlib` resolves.
    LIBC_PATH=$(find "$TC/arm-none-eabi/lib" -name 'libc.a' | head -1 || true)
    if [ -n "${LIBC_PATH:-}" ]; then
        cp "$LIBC_PATH" "$PREFIX/lib/libnewlib.a"
        info "Staged $PREFIX/lib/libnewlib.a (from $LIBC_PATH)"
    else
        # Empty-but-valid archive — `ar` rejects truly empty inputs.
        echo "// nros-board-orin-spe: newlib stub" > "$PREFIX/lib/.stub.c"
        "$TC/bin/arm-none-eabi-gcc" -c -mcpu=cortex-r5 -marm \
            -mfpu=vfpv3-d16 -mfloat-abi=hard \
            "$PREFIX/lib/.stub.c" -o "$PREFIX/lib/.stub.o"
        "$TC/bin/arm-none-eabi-ar" rcs "$PREFIX/lib/libnewlib.a" "$PREFIX/lib/.stub.o"
        rm -f "$PREFIX/lib/.stub.c" "$PREFIX/lib/.stub.o"
    fi

    info "Staging headers"
    rm -rf "$PREFIX/include/fsp" "$PREFIX/include/freertos"
    mkdir -p "$PREFIX/include/fsp" "$PREFIX/include/freertos"
    # Mirror the BSP's header layout under one prefix. Downstream
    # bindgen consumers use `-I$PREFIX/include/fsp` etc.
    cp -a "$SRC/fsp/source/include/." "$PREFIX/include/fsp/"
    cp -a "$SRC/FreeRTOSV10.4.3/FreeRTOS/Source/include/." "$PREFIX/include/freertos/"

    info "FSP staged at $PREFIX"
    echo "  archive:  $PREFIX/lib/libtegra_aon_fsp.a ($(du -h "$PREFIX/lib/libtegra_aon_fsp.a" | cut -f1))"
    echo "  newlib:   $PREFIX/lib/libnewlib.a ($(du -h "$PREFIX/lib/libnewlib.a" | cut -f1))"
    echo "  headers:  $PREFIX/include/{fsp,freertos}/"
    echo ""
    echo "Export the prefix and rebuild:"
    echo "  export NV_SPE_FSP_DIR='$PREFIX'"
    echo "  just orin_spe build"

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
    # Default to the locally-staged FSP if present.
    if [ -z "${NV_SPE_FSP_DIR:-}" ]; then
        local_stage="$(pwd)/external/spe-fsp/install"
        if [ -f "$local_stage/lib/libtegra_aon_fsp.a" ]; then
            export NV_SPE_FSP_DIR="$local_stage"
            echo "Using staged FSP: $NV_SPE_FSP_DIR"
        else
            echo "ERROR: NV_SPE_FSP_DIR is not set and no staged FSP at $local_stage."
            echo "       Run: just orin_spe bsp-download && just orin_spe bsp-build"
            echo "       For the POSIX-only mock-IVC path, use 'just orin_spe test' instead."
            exit 1
        fi
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

# Clean the staged FSP install (`external/spe-fsp/install/`) and the
# upstream demo's `out/` so `just orin_spe bsp-build` does a fresh
# rebuild. Leaves `external/spe-fsp/downloads/` (tarballs + extracted
# BSP source) in place — re-fetch with `just orin_spe bsp-download`
# only if you want to discard the source tree as well.
bsp-clean:
    #!/usr/bin/env bash
    set -e
    SRC="${SPE_BSP_SRC_DIR:-${SPE_BSP_DOWNLOAD_DIR:-external/spe-fsp/downloads}/spe-freertos-bsp}"
    if [ -d "$SRC/rt-aux-cpu-demo-fsp/out" ]; then
        rm -rf "$SRC/rt-aux-cpu-demo-fsp/out"
        echo "Removed $SRC/rt-aux-cpu-demo-fsp/out"
    fi
    rm -rf external/spe-fsp/install
    echo "FSP staged install cleaned"
