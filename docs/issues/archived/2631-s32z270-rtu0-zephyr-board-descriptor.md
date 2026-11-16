---
id: 2631
title: "No Zephyr board for the S32Z270 RTU0 R52, and a board revision in west_board broke the derived Kconfig symbol and file names"
status: resolved
type: enhancement
severity: low
area: [zephyr, build]
related: [phase-524]
found: 2026-11-16
resolved: 2026-11-16
resolved_in: "phase-524 W3"
---

**Home: [phase-524](../../roadmap/phase-524-emulator-twins.md) W3** (board
descriptors). GitHub #4014. Consumer: ASI's `s32z` image.

## What happened

nano-ros shipped no Zephyr board for the NXP S32Z270 RTU0 Cortex-R52, so ASI
carried its own package (`actuation_module/src/s32z270_zephyr_board/`:
`nros-board.toml`, a `prj.conf` copied line for line from
`fvp-aemv8r-smp/prj.conf`, a revision-D overlay) and two workarounds in
`build.sh`: `NROS_EXTRA_BOARD_PATH` (:431) to find it, and
`-DBOARD_REVISION=D` (:475).

The revision could not go in `west_board`, because two derived names took the
board id verbatim:

- `board_kconfig_symbol("s32z2xxdc2@D/s32z270/rtu0")` gave
  `BOARD_S32Z2XXDC2@D`, not a Kconfig identifier, so `RUST_SUPPORTED` would
  never be enabled for the board;
- `flatten_board_id` gave the stem `s32z2xxdc2_D_s32z270_rtu0`, a file Zephyr
  never looks for. Zephyr's `zephyr_build_string` joins the board and its
  qualifiers and puts the revision LAST (`s32z2xxdc2_s32z270_rtu0_D`; the
  revision-less `s32z2xxdc2_s32z270_rtu0` also applies). `nano_ros_use_board`
  made the same `@` -> `_` substitution for the app's own overlay.

## Fix

- `board_projection::strip_board_revision`; `flatten_board_id` and
  `board_kconfig_symbol` drop the revision (`s32z2xxdc2_s32z270_rtu0`,
  `BOARD_S32Z2XXDC2`). A descriptor that carries a revision describes that
  revision only, so its revision-less files are the ones it ships.
  `nros board new` and `nano_ros_use_board.cmake` use the same rule.
- `packages/boards/nros-board-zephyr/boards/s32z270-rtu0-zephyr/`:
  `west_board = "s32z2xxdc2@D/s32z270/rtu0"`, `armv7r-none-eabihf`, 2 MiB nros
  heap, a base `prj.conf`, and the board overlay (7 MiB `sram2` as
  `zephyr,sram`, the NETC MAC, the PHY at MDIO address 2). The console on
  uart9 and `zephyr,canbus = &can0` are not repeated: Zephyr's own revision-D
  overlay and RTU0 devicetree already state them.

## Measured

Link only: no emulator models the SoC and no S32Z board was attached.
See the PR for the build of a C++ workspace image on the board.

## Consumer cleanup

ASI drops `actuation_module/src/s32z270_zephyr_board/` (whole package),
`build.sh:427-432` (the `NROS_EXTRA_BOARD_PATH` export) and
`build.sh:472-476` (`-DBOARD_REVISION=D`). Its image row keeps
`board = "s32z270-rtu0-zephyr"`. ASI's `CONFIG_ETH_NXP_S32_RX_THREAD_STACK_SIZE`
stays in its own `prj-s32z.conf`.
