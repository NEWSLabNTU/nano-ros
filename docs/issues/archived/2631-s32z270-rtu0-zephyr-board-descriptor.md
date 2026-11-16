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
  `zephyr,sram`, the NETC MAC, the PHY at MDIO address 2), and a per-board
  conf selecting Zephyr's test PRNG. The console on
  uart9 and `zephyr,canbus = &can0` are not repeated: Zephyr's own revision-D
  overlay and RTU0 devicetree already state them.

## Measured

Link only: no emulator models the SoC and no S32Z board was attached.

`nros new <ws> --workspace --lang cpp --rmw cyclonedds`, `nros new --image
s32z --board s32z270-rtu0-zephyr`, then `nros build s32z` on Zephyr 3.7.0
(SDK 0.16.8). Zephyr reads `Board: s32z2xxdc2, Revision: D, qualifiers:
s32z270/rtu0` from the descriptor's `west_board`, applies its own
`s32z2xxdc2_s32z270_rtu0_D.overlay` and then this board's
`s32z2xxdc2_s32z270_rtu0.overlay`, and links `zephyr.elf`: RAM 3,928,964 B of
the 7 MiB `sram2` (53.5 %); `text` 1,207,228, `data` 26,434, `bss` 2,697,356.

What the link needed, and where it now lives:

- `CONFIG_TEST_RANDOM_GENERATOR=y` in `boards/s32z2xxdc2_s32z270_rtu0.conf`:
  Zephyr 3.7 has no S32Z entropy driver (`undefined reference to
  z_impl_sys_rand_get` without it), the FVP's situation.
- In the IMAGE, not the board: the C++ runtime choice (newlib +
  `CONFIG_GLIBCXX_LIBCPP`) and `CONFIG_MAIN_STACK_SIZE`. The S32ZE SoC
  `Kconfig.defconfig` defaults `MAIN_STACK_SIZE` to 1024, which wins over the
  nros module's `configdefault`, so an image states its own (ASI: 32768 in
  `prj-actuation.conf`).
- `hal_nxp` as a Zephyr module: nano-ros's `west.yml` allowlist does not carry
  it (`[zephyr_module.hal_nxp]`, 1.3 GB), so the consumer's own manifest does,
  as ASI's does. The check build passed it as `ZEPHYR_EXTRA_MODULES`.

## Consumer cleanup

ASI drops `actuation_module/src/s32z270_zephyr_board/` (whole package),
`build.sh:427-432` (the `NROS_EXTRA_BOARD_PATH` export) and
`build.sh:472-476` (`-DBOARD_REVISION=D`). Its image row keeps
`board = "s32z270-rtu0-zephyr"`. ASI's `CONFIG_ETH_NXP_S32_RX_THREAD_STACK_SIZE`
stays in its own `prj-s32z.conf`.
