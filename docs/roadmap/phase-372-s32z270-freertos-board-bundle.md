# Phase 372 — S32Z270 FreeRTOS board bundle (Cortex-R52, NETC/lwIP, Cyclone)

**Status (2026-08-22).** W1-W4 LANDED (link-completeness acceptance met);
W5 open (hardware-gated, ASI phase-4 W5.b). Filed from the ASI
reference-consumer side. Successor to phase-370.

**Landed 2026-08-22:**
* **W1** — `cmake/toolchain/arm-freertos-armcr52.cmake`, the
  `[arch.cortex-r52]` profile in `config/freertos/nros-platform.toml`
  (with the `freertos_build` guard fixed: `arm*` targets resolve profiles
  instead of silently taking the M3 legacy default), Rust target
  `armv8r-none-eabihf` (rustup-shipped rust-std, verified), and
  `nros_armv8r_cflags_env()` (issue 0657's class on a second arch: cc-rs
  builds inside cargo crates derive `-mfloat-abi=hard` from the triple with
  no `-mfpu` — gcc refuses; per-target `CFLAGS_<triple>` via corrosion env,
  wired at the nros-c / nros-cpp / runtime-crate import sites).
* **W2** — `packages/boards/nros-board-s32z270-freertos` (descriptor,
  configs seeded from the ASI hardware-proven set, first-cut public-map
  linker script with the non-cacheable NETC-BD section, weak fail-loud
  netif + tick hooks in `c/board_s32z270.c`) +
  `cmake/board/nano-ros-board-s32z270-freertos.cmake` (env-provisioned
  kernel; default `GCC/ARM_CRx_No_GIC` — whose `portASM.S` the generic
  kernel builder never compiled (M-only assumption) and which needs
  `enable_language(ASM)` or CMake DROPS the .S silently) + workspace
  board→toolchain maps + `[deploy.s32z270]` in the demo bringup + the
  emitter allowlist arm (`s32z270-freertos` → `FreertosBoard`, else the
  generated entry emits a second `main`). ACCEPTANCE MET: the C++
  cyclonedds workspace cell cross-links for ARMv8-R (`Tag_CPU_name: 8-R`,
  VFP hard-float) from a clean checkout; witness row
  `workspace-cpp-s32z270-freertos` in fixtures.toml. MPS2 sibling cell
  re-verified green (builds, boots, SPDP egress) with all shared-code
  changes.
* **W3** — confirmed reuse: the strong-symbol netif seam pre-existed; the
  bundle ships weak fail-loud defaults, ASI's `ethif_shim.c` becomes the
  strong override. Nothing new to design.
* **W4 findings** — the premise was STALE: `LWIP_IGMP=1` has been on
  family-wide since phase-97, the lan9118 driver passes all multicast
  (MCPAS), the cyclone generic `ip_mreq` join path is present, and the
  platform `net.c` multicast "stub" comment described code that was fully
  implemented (comment fixed). The QEMU cyclone cell transmits SPDP
  multicast (observed as slirp refusals — slirp cannot route 239.x).
  Full RX-side multicast interop is unreachable under slirp; it lands
  with a tap/socket-netdev lane or on hardware (W5). Heap: the QEMU cell
  budgets 3 MiB (`NROS_FREERTOS_HEAP_KB`), consistent with the 7 MiB
  Zephyr lesson; the 40-participant test also waits for W5.
* **Wall worth keeping**: a long-lived dev tree can produce
  deterministically-faulting images for this family's QEMU cells while a
  fresh worktree at the SAME commit builds bootable ones (byte-identical
  code, benign layout deltas, hardfault at boot) — the museum-binary
  class one level deeper than issue 0268. Verdicts about these cells come
  from a clean worktree or CI, never from a lived-in tree.

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

## Exploration findings (2026-08-22, from the ASI consumer side)

Surveyed the legacy ASI lane (the thing W5 retires) against what is already
in-tree; these sharpen the work items:

* **W3 is smaller than filed: the netif seam already exists.** The
  `nros_board_register_netif` / `nros_board_poll_netif` strong-symbol pair
  is live in `nros-board-freertos/c/network_glue.c` with the MPS2 LAN9118
  override as the reference implementation. The S32Z270 bundle reuses the
  contract; ASI's `ethif_shim.c` (lwIP netif over NXP `Eth_43_NETC`)
  reshapes into a strong override. Remaining W3 work is documentation +
  making sure the pair covers link-state and RX-poll cadence needs of an
  RTD driver (interrupt-driven, not polled like LAN9118).
* **W4's multicast gap located**: `nros-platform-freertos/src/net.c` —
  "UDP multicast is stubbed (returns -1)" (line ~14, impl ~358). Cyclone
  SPDP on lwIP needs a real IGMP join (`igmp_joingroup` +
  `IP_ADD_MEMBERSHIP` mapping). Reproduce on the MPS2 QEMU cell first.
* **W1's kernel-port reality: the Cortex-R52 GIC port is NOT upstream.**
  Upstream FreeRTOS-Kernel ships `GCC/ARM_CRx_No_GIC` only; S32Z270 runs
  the GIC-integrated `GCC/ARM_CR52_GIC` port from NXP's FreeRTOS
  distribution (NXP-licensed, cannot be vendored here) — and that port has
  a REAL bug ASI carries a mandatory patch for (`port.c.patch`: the IRQ
  solicited-resume path restored CPSR from SPSR_irq, corrupting the Thumb
  bit when an IRQ lands mid-Thumb-libm — sin/atan2 in the controllers).
  Consequence: the bundle must accept a CONSUMER-PROVISIONED kernel + port
  via the existing `FREERTOS_DIR`/`FREERTOS_PORT` env seam (the posix-lane
  precedent), not the nros SDK kernel source; the patch stays consumer-side
  with a provisioning script that applies it. W1's in-repo half reduces to
  the cross toolchain file (seed:
  `-mcpu=cortex-r52 -mfpu=neon-fp-armv8 -mfloat-abi=hard`, C++17 with
  exceptions+RTTI) + sdk-index arch rows.
* **W2's seeds enumerated** (all proven on hardware by the legacy lane):
  `FreeRTOSConfig.h` (1 kHz tick, RTU0 lock-step), `lwipopts.h`, and FOUR
  linker fragments the bundle's script must absorb or hook:
  `heap_in_sram.ld`, `node_stack_in_sram.ld`, `discard_unwind.ld`, and
  `netc_bd_no_cacheable.ld` — the NETC buffer descriptors MUST land in a
  non-cacheable region, which is a memory-map fact the bundle owns even
  though the driver is consumer-side. Plus `cp15_arm.S` + `board_init.c`
  early init (cache/MPU setup before the kernel).
* **PBcfg pipeline stays consumer-side**: the RTD MCAL drivers need
  post-build `*_PBcfg.c` generated from a `.mex` via S32 Config Tools; ASI
  bundles them as its private `s32ct_config` submodule with an
  `S32CT_GENERATED_DIR` override. The bundle only needs a hook point to
  compile-and-link a consumer-provided PBcfg set.
* **Known-good baseline to measure against**: the legacy lane boots on the
  X-S32Z27X-DC (RTU0 lock-step): scheduler + 1 kHz tick, NETC RX/TX over
  lwIP, Cyclone domain-2 participant, controller live at the (pre-0745)
  150 ms cadence. Acceptance W5 = parity with that, then the 30 ms tier.
* **Heap budget warning transfers**: the Zephyr S32Z bring-up needed the
  7 MiB CRAM region for Cyclone (~1 MiB is not enough); the FreeRTOS
  bundle's default heap must assume the same scale, and the
  40-participant-graph test from issue 0496 applies unchanged.
* **Recommended order**: W4 (QEMU multicast + heap) → W1 (toolchain rows,
  env-provisioned kernel) → W2 (bundle w/ stub netif link-complete) → W3
  (doc + RTD-shaped hook check) → W5 (ASI consumer swap, hardware smoke
  via ASI's `verify-b1/b2` scripts).

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
