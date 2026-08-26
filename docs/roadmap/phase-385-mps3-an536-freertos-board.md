# Phase 385 — MPS3-AN536 FreeRTOS board bundle (Cortex-R52 on QEMU, lwIP, Cyclone)

**Status (2026-08-26).** SCOPED; W0 (boot/console/toolchain spike) DONE,
W1–W6 open. Filed from the ASI reference-consumer side (its view lives in
`docs/roadmap/phase-6-emulated-r52-lane.md` in
`NEWSLabNTU/autoware-safety-island`). Sibling to phase-372, which built the
S32Z270 bundle that has never RUN.

## Why this board

phase-372 landed a Cortex-R52 bundle whose acceptance is *link-completeness*:
`nros-board-s32z270-freertos` cross-links an ARMv8-R image, and stops there,
because no emulator models the S32Z270 RTU (issue 0772 records the same
finding). So the whole ARMv8-R half of the FreeRTOS platform — the port, the
tick, the GIC, lwIP on R52, Cyclone on R52 — has **never scheduled a task
anywhere**, in this repo or in the consumer.

QEMU's `mps3-an536` is a dual Cortex-R52 board that closes that gap without
hardware, and it does so cheaply because of what it shares with what we
already have:

| an536 provides | we already have |
| --- | --- |
| Cortex-R52, AArch32 | `arm-freertos-armcr52.cmake`, `[arch.cortex-r52]`, `armv8r-none-eabihf` (phase-372 W1) |
| `lan9118` NIC at `0xe0300000` | `packages/drivers/net/lan9118-lwip` (drives the same part on MPS2) |
| QEMU board conventions | `nros-board-mps2-an385-freertos` (runner, fixture row, entry shape) |

The only substantial new code is GICv3 + a tick — and that fills the seam
`nros-board-s32z270-freertos` **already declares** (`configSETUP_TICK_INTERRUPT`
/ `configCLEAR_TICK_INTERRUPT` → weak `nros_board_setup_tick_interrupt()` /
`_clear_`), so it is not throwaway: the S32Z2 bench session needs the same
shape with a different GIC base.

## W0 — boot / console / toolchain spike. DONE (2026-08-26)

Measured by building a ~30-line R52 assembly image with the SDK toolchain and
running it on the machine (it printed `AN536-R52-BOOT-OK`). Recorded here
because each of these costs an implementation day to rediscover:

* **`-kernel <elf>` is the whole boot protocol.** QEMU loads at the ELF's own
  addresses and starts at `e_entry`; linking `.text` at DDR `0x20000000` is
  sufficient. No bootloader stub, no `-device loader`.
* **The CPU resets into `hyp32` (EL2), not SVC/PL1.** `ARM_CRx_No_GIC` is a
  PL1 port (`CPS #SVC_MODE`, IRQ/SVC banked stacks), so board startup MUST
  drop EL2 → EL1 before `vTaskStartScheduler()`. **`nros-board-s32z270-freertos`
  does not do this either** — the same gap is waiting there for hardware.
* **The console is the PER-CPU UART at `0xe7c00000`** (QEMU `serial0`). The
  four shared CMSDK UARTs at `0xe0205000`–`0xe0208000` are serial1..4 and
  print nowhere by default; writing only to `0xe0205000` looks exactly like a
  dead image.
* **There is no GICC (memory-mapped CPU interface)** — `info mtree` shows only
  `gicv3_dist` (`0xf0000000`) and `gicv3_redist_region[0]` (`0xf0100000`), so
  the port's `configEOI_ADDRESS` store cannot be a real EOI. **This needs no
  kernel fork**: the port never reads IAR — it calls `vApplicationIRQHandler()`
  with no argument, so acknowledgement is already the board's job. Point
  `configEOI_ADDRESS` at a scratch word (its trailing `STR` becomes harmless)
  and do the real `ICC_IAR1` / `ICC_EOIR1` in the handler.
* Combined with the netif being poll-mode on the MPS2 sibling
  (`nros_board_poll_netif`), **GICv3 is required for the TICK ALONE.**

The spike source is kept consumer-side at `demo/spikes/an536-boot-smoke.S`
in the ASI repo.

## Work items

* **W1 — bundle skeleton.** `packages/boards/nros-board-mps3-an536-freertos/`:
  `nros-board.toml` (names `["mps3-an536-freertos", "an536"]`, platform
  `freertos`, `supported_netstacks = ["lwip"]`, `board_crate`, entry
  signature, capabilities, `[board.cmake] toolchain_file =
  cmake/toolchain/arm-freertos-armcr52.cmake`, `[board.priority_plan]` read
  from the port, `cargo_config` runner
  `qemu-system-arm -M mps3-an536 -nographic -kernel`), crate +
  `build.rs`/`src/lib.rs`, its own `Cargo.lock`, the root `Cargo.toml`
  exclude entry (cross-only crate — phase-372's lesson), and
  `config/{FreeRTOSConfig.h,lwipopts.h,arch/cc.h,an536.ld}`.
  Reuse `[arch.cortex-r52]`; do not add a second profile.
* **W2 — EL2→EL1, GICv3, tick, scheduler.** `c/board_an536.c` + startup asm:
  vector table, EL2→EL1 drop, per-mode stacks, `VBAR`, MPU disabled to start,
  GICv3 init (dist/redist/CPU-interface via the A32 `ICC_*` CP15 encodings),
  generic-timer tick on PPI 30, `vApplicationIRQHandler` (IAR → dispatch →
  EOI), UART console at `0xe7c00000`. Overlay
  `cmake/board/nano-ros-board-mps3-an536-freertos.cmake` mirroring the
  s32z270 one (env-provisioned `FREERTOS_DIR`/`FREERTOS_PORT`,
  `enable_language(ASM)` + `portASM.S`).
  *Acceptance: two FreeRTOS tasks alternate on the console and the tick
  count advances.*
* **W3 — networking.** Strong `nros_board_register_netif` /
  `nros_board_poll_netif` over `lan9118-lwip` at base `0xe0300000`, static IP.
  *Acceptance: the host pings the guest.*
* **W4 — fixtures + CI.** `examples/fixtures.toml` witness row
  (`platform = "freertos"`, `NANO_ROS_BOARD = "mps3-an536-freertos"`, own
  `build_subdir`), `build-test-fixtures` lane membership, and a runtime cell
  so a tier actually runs it. *Acceptance: builds and boots from a clean
  checkout in CI.*
* **W5 — Cyclone entities.** *Acceptance: participant + writers/readers,
  matching what phase-370 W4 proved on MPS2.*
* **W6 — Cyclone DELIVERY out of the guest.** See the correction below: this
  is new ground, not a port. It also owns a networking decision — the
  existing an385 QEMU cells use **slirp** with a TCP zenoh locator, and
  Cyclone's SPDP wants multicast, which slirp does not carry (tap, or a
  unicast-peer config). *Acceptance: a sample crosses from the QEMU guest to
  a host CycloneDDS peer.*

## Correction carried in from the consumer's scoping

The ASI scope first called the Cyclone milestone "a port, not a bring-up", on
the strength of phase-370's status line. Read closely, that phase claims the
MPS2 cell *"builds, boots, and creates writers and readers"* — while its own
stretch goal, *"one QEMU MPS2 cyclonedds cell boots and DELIVERS locally"*, is
not claimed as met. **Cross-node DDS delivery out of a QEMU FreeRTOS guest is
therefore unproven in this repo**, which is why W5 and W6 are separate items
with separate acceptance. Worth fixing in phase-370's summary line too, since
that is what the misreading came from.

## Acceptance (phase)

1. `nros-board-mps3-an536-freertos` boots on QEMU, schedules tasks, ticks.
2. lwIP is up over the emulated `lan9118`; the host can ping it.
3. A fixture builds and boots the board in CI from a clean checkout.
4. Cyclone creates entities (W5) and delivers to a host peer (W6).
5. The consumer lane (ASI `freertos-an536`) builds and boots its controller
   image against this bundle — consumer-side acceptance lives in ASI phase-6.

## Non-goals

* SMP. an536 is dual-core; run core 0 only.
* MPU/PMSAv8 region design beyond "enough to run" — deferrable, and the S32Z2
  memory map is the one that will actually matter.
* Replacing the S32Z270 bundle. This board proves the ARMv8-R SOFTWARE stack;
  NETC, PBcfg, the licensed `GCC/ARM_CR52_GIC` port and flash/boot stay
  hardware-gated there.
