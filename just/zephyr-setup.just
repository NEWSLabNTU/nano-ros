setup *ARGS:
    #!/usr/bin/env bash
    set -e
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    ARGS="{{ARGS}}"
    # Phase 197.1 — provision the in-tree sources the Zephyr build needs the way a
    # user/CI does (`nros setup --source`, index-driven), NOT a hand
    # `git submodule update`: zenoh-pico (zenoh examples), the cyclonedds fork (the
    # patches below operate on the in-tree submodule), and px4-rs (the root
    # workspace's cargo load path-deps px4-sitl-tests). Mirrors the CI workflow so
    # local == CI. nros resolved from $NROS_CLI / PATH / ~/.nros (install.sh).
    source scripts/build/cargo.sh
    nros_bin="$(nros_cli_bin)"
    echo "[zephyr setup] provisioning sources via nros ($nros_bin)..."
    "$nros_bin" setup --source zenoh-pico --source cyclonedds-src --source px4-rs \
        --index "$(pwd)/nros-sdk-index.toml"
    if [ -d "$WORKSPACE/zephyr" ] && [[ "$ARGS" != *--force* ]]; then
        echo "Zephyr workspace already present at $WORKSPACE"
        echo "To reinstall, run: just zephyr setup --force"
    else
        NROS_ZEPHYR_MANIFEST="{{ZEPHYR_MANIFEST}}" NROS_ZEPHYR_WORKSPACE="$WORKSPACE" ./scripts/zephyr/setup.sh $ARGS
    fi
    # Phase 160.B — ensure the Cortex-A9 (qemu_cortex_a9) toolchain
    #   1. `armv7a-none-eabi` rust target (no_std core / compiler_builtins)
    #   2. `cortex-a9-rust-patch.sh`: patches Zephyr's Rust module to
    #      register the Cortex-A9 target + adds the SLCR MMU entry the
    #      Zynq-7000 eth_xlnx_gem driver needs.
    # Both are idempotent — safe to run on every `just zephyr setup`.
    echo "[zephyr setup] ensuring armv7a-none-eabi Rust target..."
    rustup target add armv7a-none-eabi
    # Phase 199.3 — version-dispatched patch set. The per-line patch sequences
    # (formerly an inline `if NROS_ZEPHYR_VERSION = 4.4 … else …` here) live in
    # scripts/zephyr/patches/<version>.sh. Adding a Zephyr line = drop a new
    # sibling there; no edit to this recipe.
    PATCHSET="./scripts/zephyr/patches/{{NROS_ZEPHYR_VERSION}}.sh"
    if [ ! -f "$PATCHSET" ]; then
        echo "[zephyr setup] ERROR: no patch set for Zephyr {{NROS_ZEPHYR_VERSION}} ($PATCHSET)." >&2
        echo "  Supported lines: $(ls ./scripts/zephyr/patches/*.sh | xargs -n1 basename | sed 's/\.sh$//' | tr '\n' ' ')" >&2
        echo "  Add scripts/zephyr/patches/{{NROS_ZEPHYR_VERSION}}.sh (see patches/README.md)." >&2
        exit 1
    fi
    echo "[zephyr setup] applying {{NROS_ZEPHYR_VERSION}} patch set ($PATCHSET)..."
    bash "$PATCHSET" "$WORKSPACE"
    # The Micro-XRCE-DDS Agent is only needed for XRCE *runtime* tests. Builds
    # that use a different RMW (e.g. the zenoh dual-line CI) can skip the heavy
    # Fast-DDS superbuild via NROS_ZEPHYR_SKIP_XRCE_AGENT.
    if [ -n "${NROS_ZEPHYR_SKIP_XRCE_AGENT:-}" ]; then
        echo "[zephyr setup] skipping Micro XRCE-DDS Agent (NROS_ZEPHYR_SKIP_XRCE_AGENT set)"
    else
        echo "[zephyr setup] ensuring Micro XRCE-DDS Agent for Zephyr XRCE tests..."
        just xrce setup
    fi

# Diagnose Zephyr install status (read-only).
[group("setup")]
doctor:
    #!/usr/bin/env bash
    set +e
    fail=0
    WORKSPACE="{{ZEPHYR_WORKSPACE}}"
    if command -v west &>/dev/null; then
        echo "  [OK] west"
    else
        echo "  [MISSING] west (run: just zephyr setup)"
        fail=1
    fi
    if [ -d "$WORKSPACE/zephyr" ]; then
        echo "  [OK] Zephyr workspace: $WORKSPACE"
    else
        echo "  [MISSING] Zephyr workspace at $WORKSPACE (run: just zephyr setup)"
        fail=1
    fi
    if rustup target list --installed 2>/dev/null | grep -q '^armv7a-none-eabi$'; then
        echo "  [OK] armv7a-none-eabi rust target (qemu_cortex_a9 DDS)"
    else
        echo "  [MISSING] armv7a-none-eabi rust target (run: just zephyr setup)"
        fail=1
    fi
    if just xrce doctor; then
        :
    else
        fail=1
    fi
    # native_sim uses NSOS (host loopback) — no TAP bridge required.
    exit $fail

# Phase 117.13 — Zephyr build smoke for the FVP Base_RevC AEMv8-R
# Cortex-A SMP target (Phase 117.10's reference platform). Reuses
# fixture — no new app required just to prove the toolchain +
# board config + nros module compile clean for AArch64. Skips
# cleanly when west / Zephyr workspace / aarch64-zephyr-elf SDK
# aren't available.
[group("examples")]
build-fvp-aemv8r:
    #!/usr/bin/env bash
    set -e
    if ! command -v west >/dev/null 2>&1; then
        echo "Zephyr/FVP skip: west not found"; exit 0
    fi
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr/FVP skip: {{ZEPHYR_WORKSPACE}} not set up"; exit 0
    fi
    sdk="$(pwd)/scripts/zephyr/sdk/zephyr-sdk-0.16.8"
    if [ ! -x "$sdk/aarch64-zephyr-elf/bin/aarch64-zephyr-elf-gcc" ]; then
        echo "Zephyr/FVP skip: aarch64-zephyr-elf toolchain missing in $sdk"
        exit 0
    fi
    bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$workspace"
    bash ./scripts/zephyr/aarch64-rust-patch.sh "$workspace"
    nros_root="$(pwd)"
    make_bin="$nros_root/third-party/make/make"
    if [ ! -x "$make_bin" ]; then
        make_bin="$(command -v make)"
    fi
    extra="$nros_root/packages/boards/nros-board-fvp-aemv8r-smp/boards/fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp.conf"
    cd "$workspace"
    export PATH="$nros_root/third-party/make:$nros_root/third-party/ninja:$PATH"
    export ZEPHYR_SDK_INSTALL_DIR="$sdk"
    west build -b "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp" \
        -d build-fvp-aemv8r-talker -p auto \
        -- -DEXTRA_CONF_FILE="$extra" -DMAKE="$make_bin"
    echo "FVP AEMv8-R Cortex-A SMP build smoke OK"

# Phase 117.14 — Zephyr build smoke for the AEMv8-R Cortex-A SMP
# nros C++ pub/sub example. Validates the Zephyr build of the
# `nros-cpp` API + codegen pipeline through to a linked
# `zephyr.elf` for the FVP target.
[group("examples")]
build-fvp-aemv8r-cyclonedds:
    #!/usr/bin/env bash
    set -e
    if ! command -v west >/dev/null 2>&1; then
        echo "Zephyr/FVP-Cyclone skip: west not found"; exit 0
    fi
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr/FVP-Cyclone skip: {{ZEPHYR_WORKSPACE}} not set up"; exit 0
    fi
    sdk="$(pwd)/scripts/zephyr/sdk/zephyr-sdk-0.16.8"
    if [ ! -x "$sdk/aarch64-zephyr-elf/bin/aarch64-zephyr-elf-gcc" ]; then
        echo "Zephyr/FVP-Cyclone skip: aarch64-zephyr-elf toolchain missing"
        exit 0
    fi
    bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$workspace"
    bash ./scripts/zephyr/aarch64-rust-patch.sh "$workspace"
    nros_root="$(pwd)"
    echo "  → host nros-codegen (cargo)"
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    nros_cargo_ensure_codegen_c
    codegen_tool="$(nros_cargo_codegen_c_bin)"
    make_bin="$nros_root/third-party/make/make"
    if [ ! -x "$make_bin" ]; then
        make_bin="$(command -v make)"
    fi
    cd "$workspace"
    export PATH="$nros_root/third-party/make:$nros_root/third-party/ninja:$PATH"
    export ZEPHYR_SDK_INSTALL_DIR="$sdk"
    echo "Building examples/zephyr/cpp/cyclonedds/talker-aemv8r for fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp..."
    west build -b "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp" \
        -d build-aemv8r-cyclonedds-talker -p auto \
        nano-ros/examples/zephyr/cpp/cyclonedds/talker-aemv8r \
        -- -D_NANO_ROS_CODEGEN_TOOL="$codegen_tool" -DMAKE="$make_bin"
    echo "Phase 117.14 FVP AEMv8-R nros-cpp + codegen build smoke OK"

# Phase 217.D.2 — Zephyr build smoke for the FVP AEMv8-R Cortex-A SMP
# nros **Rust** pub/sub example. Rust-side sibling of
# `build-fvp-aemv8r-cyclonedds` (cpp). Cyclone DDS RMW pinned via the
# example's `prj-cyclonedds.conf` + `default_rmw` from
# `packages/boards/nros-board-fvp-aemv8r-smp/board.cmake` (Phase
# 215.B — picked up by `nano_ros_use_board(fvp-aemv8r-smp)` inside the
# example's CMakeLists.txt). Skips identically to the cpp variant.
[group("examples")]
build-fvp-aemv8r-cyclonedds-rust:
    #!/usr/bin/env bash
    set -e
    if ! command -v west >/dev/null 2>&1; then
        echo "Zephyr/FVP-Cyclone-Rust skip: west not found"; exit 0
    fi
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr/FVP-Cyclone-Rust skip: {{ZEPHYR_WORKSPACE}} not set up"; exit 0
    fi
    sdk="$(pwd)/scripts/zephyr/sdk/zephyr-sdk-0.16.8"
    if [ ! -x "$sdk/aarch64-zephyr-elf/bin/aarch64-zephyr-elf-gcc" ]; then
        echo "Zephyr/FVP-Cyclone-Rust skip: aarch64-zephyr-elf toolchain missing"
        exit 0
    fi
    bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$workspace"
    bash ./scripts/zephyr/aarch64-rust-patch.sh "$workspace"
    nros_root="$(pwd)"
    echo "  → host nros-codegen (cargo)"
    source scripts/build/cargo.sh
    cargo_profile_args="$(nros_cargo_profile_arg_string)"
    nros_cargo_ensure_codegen_c
    codegen_tool="$(nros_cargo_codegen_c_bin)"
    # Phase 214.I.2 fail-loud prereq guard before `ws sync` (the codegen
    # Rust interface generation the example's `[patch.crates-io]` needs).
    nros_require_ws_sync "$codegen_tool"
    src="$nros_root/examples/zephyr/rust/cyclonedds/talker-aemv8r"
    source "$nros_root/scripts/build/codegen-stamp.sh"
    echo "  → ws sync: $src"
    NROS_REPO_DIR="$nros_root" nros_codegen_stamp_check_or_wipe "$src"
    NROS_REPO_DIR="$nros_root" "$codegen_tool" sync "$src"
    NROS_REPO_DIR="$nros_root" nros_codegen_stamp_write "$src"
    # The build path uses /opt/ros/humble/share/std_msgs for the Cyclone
    # descriptor codegen (`nros_rmw_cyclonedds_generate_from_msg`); same
    # contract as `build-one rust/talker cyclonedds`.
    export NROS_STD_MSGS_DIR="${NROS_STD_MSGS_DIR:-/opt/ros/humble/share/std_msgs}"
    export NROS_BUILTIN_INTERFACES_DIR="${NROS_BUILTIN_INTERFACES_DIR:-/opt/ros/humble/share/builtin_interfaces}"
    make_bin="$nros_root/third-party/make/make"
    if [ ! -x "$make_bin" ]; then
        make_bin="$(command -v make)"
    fi
    cd "$workspace"
    export PATH="$nros_root/third-party/make:$nros_root/third-party/ninja:$PATH"
    export ZEPHYR_SDK_INSTALL_DIR="$sdk"
    # Phase 215.B — the example's CMakeLists includes
    # `$ENV{NROS_REPO_DIR}/zephyr/cmake/nano_ros_use_board.cmake` BEFORE
    # `find_package(Zephyr)`; export the repo root so the include
    # resolves.
    export NROS_REPO_DIR="$nros_root"
    echo "Building examples/zephyr/rust/cyclonedds/talker-aemv8r for fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp..."
    west build \
        -d build-fvp-aemv8r-cyclonedds-rust-talker -p auto \
        "$src" \
        -- -D_NANO_ROS_CODEGEN_TOOL="$codegen_tool" -DMAKE="$make_bin"
    echo "Phase 217.D.2 FVP AEMv8-R nros-rust + Cyclone DDS build smoke OK"

# Phase 214.A — run the Phase 117.13 talker build on the ARM Fast Models
# `FVP_BaseR_AEMv8R` (license-gated; see `nros-sdk-index.toml`
# `[gated.arm-fvp]`). Phase 215.D.4 retargeted onto `west fvp run`
# (`scripts/west_commands/fvp.py`, registered via `zephyr/module.yml`
# `west-commands:`), which reuses the same
# `scripts/zephyr/resolve-fvp-bin.sh` resolver (ARMFVP_BIN_PATH /
# ARM_FVP_DIR / PATH), exports `ARMFVP_BIN_PATH`, then delegates to
# `west build -t run` — Zephyr's `cmake/emu/armfvp.cmake` target drives
# the canonical `boards/arm/fvp_baser_aemv8r/board.cmake` `-C` flags for
# the AEMv8-R aarch64 SMP target. UART0–3 pipe to stdout (unbuffered)
# per the board's `-C bp.pl011_uart*.out_file=-`.
[group("examples")]
run-fvp-aemv8r:
    #!/usr/bin/env bash
    set -e
    if ! command -v west >/dev/null 2>&1; then
        echo "Zephyr/FVP run skip: west not found"; exit 0
    fi
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr/FVP run skip: {{ZEPHYR_WORKSPACE}} not set up"; exit 0
    fi
    if [ ! -f "$workspace/build-fvp-aemv8r-talker/zephyr/zephyr.elf" ]; then
        echo "Zephyr/FVP run skip: build-fvp-aemv8r-talker/zephyr/zephyr.elf missing — run \`just zephyr build-fvp-aemv8r\` first"
        exit 0
    fi
    nros_root="$(pwd)"
    cd "$workspace"
    export PATH="$nros_root/third-party/make:$nros_root/third-party/ninja:$PATH"
    export ZEPHYR_SDK_INSTALL_DIR="$nros_root/scripts/zephyr/sdk/zephyr-sdk-0.16.8"
    west fvp run -d build-fvp-aemv8r-talker

# Phase 214.A — run the Phase 117.14 cpp/cyclonedds talker on the FVP.
# Phase 215.D.4 retargeted onto `west fvp run` — same resolver + west
# invocation as `run-fvp-aemv8r`, just a different build dir.
[group("examples")]
run-fvp-aemv8r-cyclonedds:
    #!/usr/bin/env bash
    set -e
    if ! command -v west >/dev/null 2>&1; then
        echo "Zephyr/FVP-Cyclone run skip: west not found"; exit 0
    fi
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr/FVP-Cyclone run skip: {{ZEPHYR_WORKSPACE}} not set up"; exit 0
    fi
    if [ ! -f "$workspace/build-aemv8r-cyclonedds-talker/zephyr/zephyr.elf" ]; then
        echo "Zephyr/FVP-Cyclone run skip: build-aemv8r-cyclonedds-talker/zephyr/zephyr.elf missing — run \`just zephyr build-fvp-aemv8r-cyclonedds\` first"
        exit 0
    fi
    nros_root="$(pwd)"
    cd "$workspace"
    export PATH="$nros_root/third-party/make:$nros_root/third-party/ninja:$PATH"
    export ZEPHYR_SDK_INSTALL_DIR="$nros_root/scripts/zephyr/sdk/zephyr-sdk-0.16.8"
    west fvp run -d build-aemv8r-cyclonedds-talker

# Phase 217.D.2 — run the Phase 217.D.1 Rust talker on the FVP. Same
# shape as `run-fvp-aemv8r-cyclonedds` over the Rust build dir produced
# by `build-fvp-aemv8r-cyclonedds-rust`. `west fvp run` reuses
# `scripts/zephyr/resolve-fvp-bin.sh` (ARMFVP_BIN_PATH / ARM_FVP_DIR /
# PATH) + `cmake/emu/armfvp.cmake`'s canonical `-C` flags.
[group("examples")]
run-fvp-aemv8r-cyclonedds-rust:
    #!/usr/bin/env bash
    set -e
    if ! command -v west >/dev/null 2>&1; then
        echo "Zephyr/FVP-Cyclone-Rust run skip: west not found"; exit 0
    fi
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr/FVP-Cyclone-Rust run skip: {{ZEPHYR_WORKSPACE}} not set up"; exit 0
    fi
    if [ ! -f "$workspace/build-fvp-aemv8r-cyclonedds-rust-talker/zephyr/zephyr.elf" ]; then
        echo "Zephyr/FVP-Cyclone-Rust run skip: build-fvp-aemv8r-cyclonedds-rust-talker/zephyr/zephyr.elf missing — run \`just zephyr build-fvp-aemv8r-cyclonedds-rust\` first"
        exit 0
    fi
    nros_root="$(pwd)"
    cd "$workspace"
    export PATH="$nros_root/third-party/make:$nros_root/third-party/ninja:$PATH"
    export ZEPHYR_SDK_INSTALL_DIR="$nros_root/scripts/zephyr/sdk/zephyr-sdk-0.16.8"
    west fvp run -d build-fvp-aemv8r-cyclonedds-rust-talker

# Phase 215.G.1 — build the minimal ASI-shaped board-import fixture
# (`packages/testing/nros-tests/fixtures/board_import_fvp`) for the FVP
# AEMv8-R target THROUGH the Phase 215.B import surface: the fixture's
# CMakeLists carries ONLY `nano_ros_use_board(fvp-aemv8r-smp)` +
# `find_package(Zephyr)` + a trivial `printk` app — no hand-rolled
# BOARD / EXTRA_CONF_FILE / DTC_OVERLAY_FILE / -DNANO_ROS_RMW. This is
# the canonical "is the board crate import surface healthy?" build.
# `nano_ros_use_board()` sets `BOARD` from `board.cmake`, so no `-b`.
# Skips cleanly when west / Zephyr workspace / aarch64 SDK are absent.
[group("examples")]
build-fvp-board-import:
    #!/usr/bin/env bash
    set -e
    if ! command -v west >/dev/null 2>&1; then
        echo "Zephyr/FVP board-import skip: west not found"; exit 0
    fi
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr/FVP board-import skip: {{ZEPHYR_WORKSPACE}} not set up"; exit 0
    fi
    sdk="$(pwd)/scripts/zephyr/sdk/zephyr-sdk-0.16.8"
    if [ ! -x "$sdk/aarch64-zephyr-elf/bin/aarch64-zephyr-elf-gcc" ]; then
        echo "Zephyr/FVP board-import skip: aarch64-zephyr-elf toolchain missing"
        exit 0
    fi
    bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$workspace"
    bash ./scripts/zephyr/aarch64-rust-patch.sh "$workspace"
    nros_root="$(pwd)"
    echo "  → host nros-codegen (cargo)"
    source scripts/build/cargo.sh
    nros_cargo_ensure_codegen_c
    codegen_tool="$(nros_cargo_codegen_c_bin)"
    make_bin="$nros_root/third-party/make/make"
    if [ ! -x "$make_bin" ]; then
        make_bin="$(command -v make)"
    fi
    src="$nros_root/packages/testing/nros-tests/fixtures/board_import_fvp"
    cd "$workspace"
    export PATH="$nros_root/third-party/make:$nros_root/third-party/ninja:$PATH"
    export ZEPHYR_SDK_INSTALL_DIR="$sdk"
    # Phase 215.B — the fixture's CMakeLists includes
    # `$ENV{NROS_REPO_DIR}/zephyr/cmake/nano_ros_use_board.cmake` BEFORE
    # `find_package(Zephyr)`; export the repo root so the include resolves.
    export NROS_REPO_DIR="$nros_root"
    echo "Building board-import fixture via nano_ros_use_board(fvp-aemv8r-smp)..."
    west build \
        -d build-fvp-board-import -p auto \
        "$src" \
        -- -D_NANO_ROS_CODEGEN_TOOL="$codegen_tool" -DMAKE="$make_bin"
    echo "Phase 215.G.1 FVP board-import fixture build OK"

# Phase 215.G.1 — run the board-import fixture on the FVP THROUGH the
# import surface (`west fvp run` reads `NROS_BOARD_RUNNER=armfvp` from
# the CMakeCache that `nano_ros_use_board()` cached). Same resolver +
# west delegation as the other `run-fvp-*` recipes; different build dir.
# The Phase 215.G integration test (`phase215_g_fvp_smoke`) drives this
# recipe and scrapes the `nros: smoke ok` UART line.
[group("examples")]
run-fvp-board-import:
    #!/usr/bin/env bash
    set -e
    if ! command -v west >/dev/null 2>&1; then
        echo "Zephyr/FVP board-import run skip: west not found"; exit 0
    fi
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr/FVP board-import run skip: {{ZEPHYR_WORKSPACE}} not set up"; exit 0
    fi
    if [ ! -f "$workspace/build-fvp-board-import/zephyr/zephyr.elf" ]; then
        echo "Zephyr/FVP board-import run skip: build-fvp-board-import/zephyr/zephyr.elf missing — run \`just zephyr build-fvp-board-import\` first"
        exit 0
    fi
    nros_root="$(pwd)"
    cd "$workspace"
    export PATH="$nros_root/third-party/make:$nros_root/third-party/ninja:$PATH"
    export ZEPHYR_SDK_INSTALL_DIR="$nros_root/scripts/zephyr/sdk/zephyr-sdk-0.16.8"
    west fvp run -d build-fvp-board-import

# Phase 117.13 — Zephyr build smoke for the NXP X-S32Z270-DC (DC2)
# RTU0 Cortex-R52 target (Phase 117.11's reference platform). Reuses
# cleanly when west / Zephyr workspace / arm-zephyr-eabi SDK aren't
# available.
[group("examples")]
build-s32z:
    #!/usr/bin/env bash
    set -e
    if ! command -v west >/dev/null 2>&1; then
        echo "Zephyr/S32Z skip: west not found"; exit 0
    fi
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr/S32Z skip: {{ZEPHYR_WORKSPACE}} not set up"; exit 0
    fi
    sdk="$(pwd)/scripts/zephyr/sdk/zephyr-sdk-0.16.8"
    if [ ! -x "$sdk/arm-zephyr-eabi/bin/arm-zephyr-eabi-gcc" ]; then
        echo "Zephyr/S32Z skip: arm-zephyr-eabi toolchain missing in $sdk"
        exit 0
    fi
    bash ./scripts/zephyr/cortex-a9-rust-patch.sh "$workspace"
    bash ./scripts/zephyr/aarch64-rust-patch.sh "$workspace"
    bash ./scripts/zephyr/cortex-r-rust-patch.sh "$workspace"
    nros_root="$(pwd)"
    make_bin="$nros_root/third-party/make/make"
    if [ ! -x "$make_bin" ]; then
        make_bin="$(command -v make)"
    fi
    extra="$nros_root/packages/boards/nros-board-s32z270dc2-r52/boards/s32z2xxdc2_s32z270_rtu0_D.conf"
    cd "$workspace"
    export PATH="$nros_root/third-party/make:$nros_root/third-party/ninja:$PATH"
    export ZEPHYR_SDK_INSTALL_DIR="$sdk"
    # Use the zenoh talker rather than the DDS one — the eval board
    # has only 1 MiB SRAM, which Rust core overflows by
    # ~100 KiB. zenoh-pico's footprint fits and exercises the same
    # toolchain + nros module + Rust API path the build smoke needs
    # to validate. Real Cyclone DDS deployments live on devices with
    # 4+ MiB SRAM (or LPDDR-backed regions) — out of scope for a
    # build-only Phase 117.13 gate.
    # Best-effort: validates the Rust + nros toolchain integration
    # for the Cortex-R52 target (Phase 117.11/13). Two pre-existing
    # issues block a clean full-link build, both outside Phase
    # 117.11's scope:
    #
    #   1. The S32Z RTU0 has only 1 MiB SRAM (post-XIP=n) so the
    #      full nros + zenoh-pico image overflows at link time.
    #   2. zpico-sys's cbindgen-generated `uintptr_t` mismatches the
    #      `size_t` in `zpico.c` under arm-zephyr-eabi picolibc (a
    #      cbindgen-config issue tracked separately).
    #
    # Both gates fall after Cargo successfully compiles the Rust
    # application + nros + zpico-sys for `armv7r-none-eabihf` — which
    # is the actual Phase 117.13 acceptance bar (toolchain plumbing
    # works for the new board target).
    set +e
    # Unified log dir (test-logs/) instead of a repo-root scatter.
    mkdir -p "$nros_root/test-logs"
    s32z_log="$nros_root/test-logs/s32z-build.log"
    west build -b "s32z2xxdc2@D/s32z270/rtu0" \
        -d build-s32z-talker -p auto \
        -- -DEXTRA_CONF_FILE="$extra" -DMAKE="$make_bin" \
        > "$s32z_log" 2>&1
    rc=$?
    set -e
    rust_release="$workspace/build-s32z-talker/rust/target/armv7r-none-eabihf/release/librustapp.a"
    rust_debug="$workspace/build-s32z-talker/rust/target/armv7r-none-eabihf/debug/librustapp.a"
    if [ "$rc" -eq 0 ]; then
        echo "S32Z R52 build smoke OK (full link succeeded)"
    elif [ -f "$rust_release" ] || [ -f "$rust_debug" ]; then
        echo "S32Z R52 build smoke OK (Rust + nros compile path verified;"
        echo "  full link is best-effort — pre-existing SRAM overflow +"
        echo "  zpico-sys cbindgen typedef issue, see Phase 117 docs)."
    else
        echo "S32Z R52 build smoke FAILED — Rust + nros compile path broken"
        echo "  Build log: $s32z_log"
        exit 1
    fi
