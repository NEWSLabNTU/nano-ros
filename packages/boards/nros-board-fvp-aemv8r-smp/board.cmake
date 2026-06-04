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
