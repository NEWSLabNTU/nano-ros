# nros-board-s32z270dc2-r52 — sidecar cmake manifest
#
# phase-292 W3.a — schema-conformant manifest for the NXP X-S32Z270-DC (DC2)
# RTU0 Cortex-R52 Zephyr board (D-rev silicon), so
# `nano_ros_use_board(s32z270dc2-r52)` layers prj.conf + the per-board
# Kconfig fragment + DTS overlay + default RMW into a downstream Zephyr
# app's build. This closes the ASI gap G7 (autoware-safety-island phase-3
# W4.a): the `zephyr-s32z` platform gets the same one-line board import the
# FVP path already has, replacing the hand-glued EXTRA_DTC_OVERLAY wiring.
#
# NOTE (ASI): the crate targets the STANDARD NXP eval board (LPUART0
# console, PHY@7). The ARM Automotive Kit variant ASI ships (UART9 console,
# PHY@2, CAN) layers its own overlay ON TOP — `nano_ros_use_board()` appends
# the crate's files first; the consumer appends its kit overlay after the
# call, exactly like the fvp_entry appends its prj fragments.
#
# Schema reference: docs/reference/board-cmake-schema.md
# Phase docs:      docs/roadmap/phase-215-board-crate-as-importable-unit.md
#                  docs/roadmap/phase-292-asi-reference-consumer-revisit.md

# Zephyr board id (hwv2 `<board>[@rev]/<soc>/<variant>` form). Zephyr 3.7
# renamed the 3.5-era `s32z270dc2_rtu0_r52` to this — consumers still
# carrying the legacy id will not resolve on the 3.7 line.
set(NROS_BOARD_ZEPHYR_ID
    "s32z2xxdc2@D/s32z270/rtu0")

# Zephyr-SDK toolchain ABI target (Cortex-R52, aarch32).
set(NROS_BOARD_TOOLCHAIN
    "arm-zephyr-eabi")

# No license-gated SDK packages — real hardware, flashed via the NXP S32
# debug probe / TRACE32 (see the stock Zephyr board.cmake's runners).
set(NROS_BOARD_GATED_PKGS
    "")

# Default RMW backend — Cyclone DDS, matching the ASI reference consumer.
# NOTE: the RTU0 has only 1 MiB SRAM; the per-board fragment trims heap +
# net buffers to make a small Cyclone image fit (build-proof scope; runtime
# tuning is hardware territory).
set(NROS_BOARD_DEFAULT_RMW
    "cyclonedds")

# Default transport — ENETC PSI0 ethernet.
set(NROS_BOARD_DEFAULT_TRANSPORT
    "ethernet")

# Zephyr `west` runner — hardware board: the stock Zephyr board.cmake
# registers `nxp_s32dbg` + `trace32`; `west flash` picks between them.
# There is no emulator runner (`west fvp run` refuses non-armfvp runners).
set(NROS_BOARD_RUNNER
    "nxp_s32dbg")

# Base `prj.conf` carried by the board crate. Layered into
# `EXTRA_CONF_FILE` ahead of the consumer's own fragments.
set(NROS_BOARD_PRJ_CONF
    "${CMAKE_CURRENT_LIST_DIR}/prj.conf")

# Per-board hwv2 Kconfig fragment (slash-flattened board id + revision).
set(NROS_BOARD_BOARD_CONF
    "${CMAKE_CURRENT_LIST_DIR}/boards/s32z2xxdc2_s32z270_rtu0_D.conf")

# Per-board DTS overlay.
set(NROS_BOARD_BOARD_OVERLAY
    "${CMAKE_CURRENT_LIST_DIR}/boards/s32z2xxdc2_s32z270_rtu0_D.overlay")

# ---------------------------------------------------------------------------
# Phase 215.J.1 — downstream consumer provisioning contract
# (`nros setup board s32z270dc2-r52 --zephyr-workspace <dir>`).
# ---------------------------------------------------------------------------

# Zephyr support line.
set(NROS_BOARD_ZEPHYR_LINE
    "3.7")

# The reference consumer (ASI) is C++-only on this board; the crate does
# not require the Rust language module. (The base prj.conf carries no
# CONFIG_RUST rows — the phase-292 W1.a lesson from the FVP crate.)
set(NROS_BOARD_REQUIRES_RUST
    "n")

set(NROS_BOARD_RUST_TARGETS
    "")

# Cyclone RMW source tree (index-driven).
set(NROS_BOARD_RMW_SOURCE
    "cyclonedds-src")
