# Phase 372 — S32Z270 FreeRTOS board bundle (Cortex-R52, NETC/lwIP, Cyclone)

**Status (2026-08-21).** OPEN — filed from the ASI reference-consumer side
(ASI phase-4 W5.b is the consuming half). Successor to phase-370: the
freertos-posix board and the first embedded Cyclone×FreeRTOS cell (QEMU
MPS2, W4) are landed; this phase brings the same lane to ASI's second
hardware target so the last vendored-CycloneDDS consumer retires.

## Why

ASI's `freertos-s32z2` target still hand-builds the vendored CycloneDDS
fork with an ASI-owned Cortex-R52 toolchain file, NXP-RTD glue and an
imperative boot. Phase-370 proved Cyclone-on-FreeRTOS end to end on QEMU;
S32Z270 is the hardware landing. The Zephyr side of this board is already
in-tree (`docs/reference/zephyr-armv8r-setup.md`, phase-292 W3 Cyclone
build proof); the FreeRTOS side has NO bundle — the old
`s32z270dc2-r52` scaffold was deleted in phase-337 W7.b as contributing
zero.

## Work items

* **W1 — Cortex-R52 FreeRTOS cross profile**: toolchain file (the tree
  has only `arm-freertos-armcm3.cmake`), kernel port `GCC/ARM_CR52`,
  `[board.*]`/arch profile rows in `nros-sdk-index.toml`. ASI's retired
  `actuation_module/freertos_s32z2/cmake/arm-cortex-r52.cmake` is the
  known-good starting point.
* **W2 — `nros-board-s32z270-freertos` bundle**: descriptor
  (`platform = freertos`, netstack lwip, cross), `FreeRTOSConfig.h` +
  `lwipopts.h` (seed from ASI's proven RTU0 configs), linker script for
  the RTU memory map — the 7 MiB CRAM lesson from the Zephyr bring-up
  applies (Cyclone does not fit ~1 MiB), `.init_array` KEEP + walk per
  the phase-370 W4 pattern (issue 0733).
* **W3 — netif seam**: the NETC ethernet driver comes from the NXP RTD
  (NXP Confidential license) and CANNOT live in this repo. Define the
  board hook the consumer implements (`nros_board_freertos_netif_init`
  strong-symbol shape, mirroring the lan9118 wiring on MPS2) so the
  bundle carries everything generic and ASI carries only the licensed
  glue.
* **W4 — Cyclone-on-lwIP hardening on the QEMU cell first**: whatever
  W2/W3 shake loose lands against the MPS2 cell (cheap repro) before
  hardware — multicast/IGMP on lwIP (the platform `net.c` join is still
  stubbed), `kEmbeddedCycloneConfig` heap budget under a 40-participant
  Autoware graph.
* **W5 — consumer validation**: ASI switches `--platform freertos-s32z2`
  onto the bundle (ASI phase-4 W5.b), deletes its vendored `cyclonedds/`
  submodule, hand toolchain and `build-cdds-target.sh`; on-target smoke
  on the S32Z270DC2 board. Hardware-gated; walls filed here.

## Non-goals

* Zephyr S32Z parity (tracked separately; the Zephyr board bundle path
  already exists for FVP and the S32Z Zephyr consumption is ASI phase-3
  W4).
* Other RMWs on this board — cyclonedds only (wire compat with the
  Autoware side is the point).

## Acceptance

* Bundle + cross profile build a C++ cyclonedds workspace cell for
  Cortex-R52 from a clean checkout (link-complete against a stub netif).
* MPS2 QEMU cell still green after the shared-code changes.
* ASI `freertos-s32z2` builds via the bundle with the vendored
  middleware deleted (consumer acceptance in ASI phase-4 W5.b);
  on-target pub/sub smoke when hardware is available.
