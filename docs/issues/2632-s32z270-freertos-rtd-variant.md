---
id: 2632
title: "s32z270-freertos hardware needs the NXP RTD port and NETC glue from consumer env; it should be a board variant"
status: open
type: enhancement
severity: medium
area: [freertos, boards, build]
related: [phase-524, phase-372, 0772]
found: 2026-11-16
---

GitHub: NEWSLabNTU/nano-ros#4015

**Home: [phase-524](../roadmap/phase-524-emulator-twins.md) W3** (board
descriptors). Consumer: ASI's FreeRTOS lane on S32Z270 hardware.

## What happens

`s32z270-freertos` forces `FREERTOS_PORT = GCC_ARM_CRX_NOGIC` (the in-tree
port, so a clean checkout links) and has no NETC netif. Hardware needs NXP's
`GCC/ARM_CR52_GIC` port and the RTD NETC driver, both NXP-licensed. ASI
carries all of it:

- `scripts/provision-nxp-freertos.sh`, which stages the NXP kernel and sets
  `FREERTOS_DIR` / `FREERTOS_PORT=GCC/ARM_CR52_GIC` in the environment;
- `actuation_module/src/s32z2_board_glue/` (a stub without `S32_RTD_PATH`);
- a pthread shim switching to the RTD `xTaskCreate*Fpu` variants
  (`asi_common/CMakeLists.txt:94-99`, `ASI_S32Z2_NXP_PORT`);
- `vendor_patched/*.patch` over the RTD sources.

The `*Fpu` switch is not ASI's alone: the NXP port enables the FPU per task,
lazily, through a save area only `xTaskCreateFpu` / `xTaskCreateStaticFpu`
set up, and nano-ros's own sched-task, level-task and platform tasks were
created with the plain `xTaskCreate*`.

## Done (2026-11-16)

- **A board variant, `s32z270-freertos-rtd`**: a second `[[board]]` in
  `nros-board-s32z270-freertos/nros-board.toml`, its own CMake module
  (`cmake/board/nano-ros-board-s32z270-freertos-rtd.cmake`) over the base one,
  port `GCC/ARM_CR52_GIC` as a board fact (not overridable), and a
  `config-rtd/` FreeRTOSConfig directory.
- **The licensed trees are named, not guessed**: `[gated.nxp-freertos-s32ze]`
  (`NXP_FREERTOS_S32ZE_DIR`, the patched kernel holding `include/` and
  `portable/GCC/ARM_CR52_GIC`) and `[gated.nxp-rtd-s32ze]` (`S32_RTD_PATH`) in
  `nros-sdk-index.toml`, and `[board.s32z270-freertos-rtd]` lists them. The
  module refuses a configure without either, naming the package and variable.
- **nano-ros's own tasks use the port's FPU creators** on that variant:
  `nros-board-freertos/c/nros_freertos_task_create.h`, keyed on
  `nrosconfigTASK_CREATE_FPU` in the board's FreeRTOSConfig.h, used by
  `freertos_task_glue.c`, `freertos_run_sched_tasks.c` and
  `nros-platform-freertos/src/platform.c`. Every other board gets the plain
  creators, unchanged.

Checked here (no RTD, no NXP kernel, no S32Z board on this host): the three
TUs compile for Cortex-R52 against the in-tree kernel headers with the base
config (undefined: `xTaskCreate`, `xTaskCreateStatic`) and with `config-rtd/`
(undefined: `xTaskCreateFpu`, `xTaskCreateStaticFpu`); the variant's configure
refuses without `NXP_FREERTOS_S32ZE_DIR`.

## Still open

1. **The NETC netif glue.** It is written against the RTD Eth driver
   (`Eth_43_NETC`, and lwIP's RTD `ethif` port), and needs the RTD on a host to
   write and build. Until it lands, the variant keeps the board's weak
   fail-loud `nros_board_register_netif` / `nros_board_poll_netif`, so a
   consumer's strong overrides (ASI's glue) still link.
2. **The port patch** (ASI's `vendor_patched/port.c.patch`, the Thumb-bit
   resume fix): the variant expects an already patched kernel. Carrying the
   patch in nano-ros needs a licence answer, since its context lines are NXP's.
3. **A link of a full image on the variant**, and the hardware run: both need
   the NXP trees.
