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

# Issue #216 — aggregate FVP build sweep. The individual FVP lanes are
# manual-only (no CI consumer; each skips cleanly when west / the Zephyr
# workspace / the aarch64 SDK is missing), which is exactly how the rust
# lane rotted silently to E0463 for weeks. One verb for a periodic local
# sweep: `just zephyr build-fvp-all`. (The historical `build-fvp-aemv8r`
# base lane was retired by #217: unbuildable since phase-221 dropped its
# west source arg, its app long gone, and its "rust compiles for AArch64"
# purpose is covered by the cyclonedds-rust lane. Phase-292 W1.a adds the
# workspace-Entry FVP lane here when it lands.)
[group("examples")]
build-fvp-all: build-fvp-aemv8r-cyclonedds build-fvp-aemv8r-cyclonedds-rust build-fvp-ws-entry

# phase-292 W1.a — the FVP WORKSPACE C++ RT-tiers Entry
# (examples/workspaces/ws-realtime-cpp-fvp/src/fvp_entry): proves the ASI
# reference consumer's exact shape in-tree — `nano_ros_use_board(fvp-aemv8r-smp)`
# + `find_package(nano_ros)` (287-W6 verbs) + one
# `nano_ros_add_executable(BOARD zephyr LAUNCH … TYPED)` in a single west
# configure. NO `-b` flag: the board id comes from the board crate. Skips
# identically to the other FVP lanes.
[group("examples")]
build-fvp-ws-entry:
    #!/usr/bin/env bash
    set -e
    if ! command -v west >/dev/null 2>&1; then
        echo "Zephyr/FVP-ws-entry skip: west not found"; exit 0
    fi
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr/FVP-ws-entry skip: {{ZEPHYR_WORKSPACE}} not set up"; exit 0
    fi
    sdk="$(pwd)/scripts/zephyr/sdk/zephyr-sdk-0.16.8"
    if [ ! -x "$sdk/aarch64-zephyr-elf/bin/aarch64-zephyr-elf-gcc" ]; then
        echo "Zephyr/FVP-ws-entry skip: aarch64-zephyr-elf toolchain missing"
        exit 0
    fi
    nros_root="$(pwd)"
    echo "  → host nros-codegen (cargo)"
    source scripts/build/cargo.sh
    nros_cargo_ensure_codegen_c
    codegen_tool="$(nros_cargo_codegen_c_bin)"
    make_bin="$nros_root/third-party/make/make"
    if [ ! -x "$make_bin" ]; then
        make_bin="$(command -v make)"
    fi
    cd "$workspace"
    export PATH="$nros_root/third-party/make:$nros_root/third-party/ninja:$PATH"
    export ZEPHYR_SDK_INSTALL_DIR="$sdk"
    export NROS_REPO_DIR="$nros_root"
    echo "Building examples/workspaces/ws-realtime-cpp-fvp/src/fvp_entry (board from nano_ros_use_board)..."
    west build \
        -d build-fvp-ws-entry -p auto \
        "$nros_root/examples/workspaces/ws-realtime-cpp-fvp/src/fvp_entry" \
        -- -D_NANO_ROS_CODEGEN_TOOL="$codegen_tool" -DMAKE="$make_bin"
    echo "phase-292 W1.a FVP AEMv8-R workspace-Entry (cpp/cyclonedds) build smoke OK"

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
    # phase-292 W2 — this example uses `find_package(Zephyr)` + `-b` directly
    # (the module-verb path, not `nano_ros_use_board`), so it never received
    # the cyclone RMW snippet Kconfig. Its image then built with no cyclone
    # worker-thread stacks / pthread pools / waitset socketpair and crashed on
    # SMP-4 inside dds_create_participant (invisible while the FVP runtime lane
    # was skip-only). Layer the snippet conf on via EXTRA_CONF_FILE — same
    # fragments `nano_ros_use_board` merges for the workspace/rust lanes, no
    # duplication, no disturbance to the module-verb `nros_generate_interfaces`.
    west build -b "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp" \
        -d build-aemv8r-cyclonedds-talker -p auto \
        nano-ros/examples/zephyr/cpp/cyclonedds/talker-aemv8r \
        -- -D_NANO_ROS_CODEGEN_TOOL="$codegen_tool" -DMAKE="$make_bin" \
        -DEXTRA_CONF_FILE="$nros_root/zephyr/snippets/nros-cyclonedds/cyclonedds.conf"
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

# phase-292 W3.a — S32Z270-DC RTU0 board-crate IMPORT smoke: builds the
# minimal `board_import_s32z` fixture via `nano_ros_use_board(s32z270dc2-r52)`
# (the ASI zephyr-s32z consumption surface). Sibling of
# `build-fvp-board-import`. Skips when west / workspace / arm-zephyr-eabi
# SDK are missing.
[group("examples")]
build-s32z-board-import:
    #!/usr/bin/env bash
    set -e
    if ! command -v west >/dev/null 2>&1; then
        echo "Zephyr/S32Z board-import skip: west not found"; exit 0
    fi
    workspace="$(realpath {{ZEPHYR_WORKSPACE}} 2>/dev/null || echo "")"
    if [ -z "$workspace" ] || [ ! -d "$workspace/zephyr" ]; then
        echo "Zephyr/S32Z board-import skip: {{ZEPHYR_WORKSPACE}} not set up"; exit 0
    fi
    sdk="$(pwd)/scripts/zephyr/sdk/zephyr-sdk-0.16.8"
    if [ ! -x "$sdk/arm-zephyr-eabi/bin/arm-zephyr-eabi-gcc" ]; then
        echo "Zephyr/S32Z board-import skip: arm-zephyr-eabi toolchain missing"
        exit 0
    fi
    nros_root="$(pwd)"
    source scripts/build/cargo.sh
    nros_cargo_ensure_codegen_c
    codegen_tool="$(nros_cargo_codegen_c_bin)"
    make_bin="$nros_root/third-party/make/make"
    if [ ! -x "$make_bin" ]; then
        make_bin="$(command -v make)"
    fi
    src="$nros_root/packages/testing/nros-tests/fixtures/board_import_s32z"
    cd "$workspace"
    export PATH="$nros_root/third-party/make:$nros_root/third-party/ninja:$PATH"
    export ZEPHYR_SDK_INSTALL_DIR="$sdk"
    export NROS_REPO_DIR="$nros_root"
    echo "Building board-import fixture via nano_ros_use_board(s32z270dc2-r52)..."
    west build \
        -d build-s32z-board-import -p auto \
        "$src" \
        -- -D_NANO_ROS_CODEGEN_TOOL="$codegen_tool" -DMAKE="$make_bin"
    echo "phase-292 W3.a S32Z board-import fixture build OK"

# phase-292 W3.a — the historical `build-s32z` rust-talker smoke was RETIRED
# here: it was sourceless since phase-221 (the #217 class — `west build` with
# no app arg, alive only on museum build-dir caches), its original app is
# gone, and the board's reference consumer (ASI) is C++-only
# (NROS_BOARD_REQUIRES_RUST=n). `build-s32z-board-import` above is the
# canonical S32Z build proof.
