# nros-board-s32z270dc2-r52

NXP X-S32Z270-DC (DC2) evaluation board, RTU0 Cortex-R52 cores under
Zephyr. Production-silicon target for the Cyclone DDS RMW backend on the
Autoware safety-island stack (the ASI `zephyr-s32z` platform consumes this
crate — phase-292 W3.a).

## Import (the one-line consumer shape)

```cmake
# BEFORE find_package(Zephyr):
include($ENV{NROS_REPO_DIR}/zephyr/cmake/nano_ros_use_board.cmake)
nano_ros_use_board(s32z270dc2-r52)
```

That layers, from `board.cmake`: the Zephyr 3.7 board id
`s32z2xxdc2@D/s32z270/rtu0` (the 3.5-era `s32z270dc2_rtu0_r52` name no
longer resolves), the base `prj.conf`, the per-board Kconfig fragment
(Cyclone RMW + its `CPP`/`POSIX` deps, ENETC RX stack, 1-MiB-aware
sizing), and the DTS overlay (7 MiB `sram2` CRAM — the ASI
hardware-validated memory map — LPUART0 console + pinctrl, ENETC PSI0).

The base prj.conf is language-neutral: the consumer picks its API surface
(`CONFIG_NROS_C_API` / `CONFIG_NROS_CPP_API`; Rust apps layer the crate's
`prj-rust.conf`). The ARM Automotive Kit variant (UART9 console, PHY@2,
CAN) layers its own overlay after the `nano_ros_use_board()` call.

## Build smoke

```sh
just zephyr setup                   # one-time: workspace + SDK
just zephyr build-s32z-board-import # phase-292 W3.a import smoke
```

## Runtime

Hardware-gated:
- NXP X-S32Z270-DC (DC2) evaluation board (or the ARM Automotive Kit).
- NXP S32 Design Studio + S32 Debug Probe (or Lauterbach Trace32) for
  flashing: `west flash --runner nxp_s32dbg` (the crate's cached default)
  or `--runner trace32`.

Build smoke is the gating check; runtime / interop validation against
`rmw_cyclonedds_cpp` peers stays with the ASI hardware (ASI phase-3 W4).

## Status

- Phase 117.11 — config + skeleton landed.
- phase-292 W3.a — `board.cmake` sidecar added (`nano_ros_use_board()`
  works); Cyclone RMW default moved into the board fragment; rust rows
  split to `prj-rust.conf`; ASI sram2/pinctrl fixes folded in; the old
  sourceless `build-s32z` rust smoke retired in favor of the import smoke.
