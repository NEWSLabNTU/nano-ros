# nros-board-fvp-aemv8r-smp — sidecar cmake manifest
#
# Phase 215.A.2 — schema-conformant manifest for the
# `fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp` Zephyr board (Cortex-A
# AArch64 SMP under the Arm Fast Models AEMv8-R FVP). Consumed by
# `nano_ros_use_board(fvp-aemv8r-smp)` (Phase 215.B) to layer prj.conf
# + per-board Kconfig + DTS overlay + default RMW + runner into a
# downstream Zephyr app's build.
#
# Schema reference: docs/reference/board-cmake-schema.md
# Phase doc:       docs/roadmap/phase-215-board-crate-as-importable-unit.md

# Zephyr board id (hwv2 `<board>/<soc>/<variant>` form).
set(NROS_BOARD_ZEPHYR_ID
    "fvp_baser_aemv8r/fvp_aemv8r_aarch64/smp")

# Zephyr-SDK toolchain ABI target.
set(NROS_BOARD_TOOLCHAIN
    "aarch64-zephyr-elf")

# License-gated SDK packages keyed on `nros-sdk-index.toml` `[gated.*]`.
# Arm Fast Models is license-gated (Arm Development Studio or the
# standalone FVP package); `nros doctor --board fvp-aemv8r-smp` cross-
# checks `ARM_FVP_DIR` per the `[gated.arm-fvp]` entry.
set(NROS_BOARD_GATED_PKGS
    "arm-fvp")

# Default RMW backend — Phase 117 reference for Cyclone DDS on the
# Autoware safety-island stack. Downstream consumers can override via
# `-DNANO_ROS_RMW=<rmw>` before `nano_ros_use_board()`.
set(NROS_BOARD_DEFAULT_RMW
    "cyclonedds")

# Default transport — matches the crate's default Cargo feature
# (`ethernet`). The FVP's networking is wired through Zephyr's
# `ethernet` L2 over the model's virtual NIC.
set(NROS_BOARD_DEFAULT_TRANSPORT
    "ethernet")

# Zephyr `west` runner — `armfvp` invokes
# `zephyr/cmake/emu/armfvp.cmake` which spawns
# `FVP_BaseR_AEMv8R` with UART → stdout. Phase 215.D's `west fvp`
# extension reads this from `CMakeCache.txt`.
set(NROS_BOARD_RUNNER
    "armfvp")

# Base `prj.conf` carried by the board crate. Layered into
# `EXTRA_CONF_FILE` ahead of the consumer's own prj.conf.
set(NROS_BOARD_PRJ_CONF
    "${CMAKE_CURRENT_LIST_DIR}/prj.conf")

# Per-board hwv2 Kconfig fragment. Filename is the slash-flattened
# Zephyr board id (`/` → `_`).
set(NROS_BOARD_BOARD_CONF
    "${CMAKE_CURRENT_LIST_DIR}/boards/fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp.conf")

# Per-board DTS overlay.
set(NROS_BOARD_BOARD_OVERLAY
    "${CMAKE_CURRENT_LIST_DIR}/boards/fvp_baser_aemv8r_fvp_aemv8r_aarch64_smp.overlay")

# ---------------------------------------------------------------------------
# Phase 215.J.1 — downstream Zephyr consumer provisioning contract.
#
# `nano_ros_use_board()` layers the board *config* (above), but an
# `import:false` downstream consumer does NOT inherit nano-ros's *toolchain
# provisioning* (zephyr patches × zephyr-lang-rust × cyclonedds source).
# These fields drive `nros setup board fvp-aemv8r-smp --zephyr-workspace <dir>`
# (Phase 215.J.2) so the board crate is the single source of truth for what a
# consumer's zephyr tree needs. Schema: docs/reference/board-cmake-schema.md.
# ---------------------------------------------------------------------------

# Zephyr support line — selects `scripts/zephyr/patches/<line>.sh`, the
# parameterized (workspace-dir = $1) patch set applied to the consumer's tree.
set(NROS_BOARD_ZEPHYR_LINE
    "3.7")

# Whether the board requires the Rust language module (`CONFIG_RUST`). When
# `y`, `nros setup board` adds the rust targets below + ensures `RUST_SUPPORTED`
# is enabled for the board's arch (Phase 215.J.4 board Kconfig overlay module).
set(NROS_BOARD_REQUIRES_RUST
    "y")

# rustup target triple(s) the board's Zephyr build compiles the nros staticlib
# for. AArch64 AEMv8-R: `zephyr-lang-rust`'s `_rust_map_target` returns
# `aarch64-unknown-none` (see scripts/zephyr/aarch64-rust-patch.sh). Semicolon
# list — `nros setup board` runs `rustup target add` for each.
set(NROS_BOARD_RUST_TARGETS
    "aarch64-unknown-none")

# `nros-sdk-index.toml` `[source.*]` name for the board's RMW source tree.
# Index-driven (`nros setup --source cyclonedds-src`) — never a hardcoded path
# or a hand `git submodule update`.
set(NROS_BOARD_RMW_SOURCE
    "cyclonedds-src")

# Phase 215.J.4 — board-shipped Kconfig overlay MODULE that enables
# `RUST_SUPPORTED` for this board's arch WITHOUT mutating the consumer's
# `zephyr-lang-rust` tree (a cross-file `default y if <board>` extension of the
# existing `config RUST_SUPPORTED`). The module root is the dir below (it
# contains `zephyr/module.yml`). Consumers put it on `ZEPHYR_EXTRA_MODULES`;
# `nano_ros_use_board()` appends it automatically.
set(NROS_BOARD_RUST_SUPPORT_MODULE
    "${CMAKE_CURRENT_LIST_DIR}/zephyr-rust-support")
