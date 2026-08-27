# Phase 385 — MPS3-AN536 FreeRTOS board bundle (Cortex-R52 on QEMU, lwIP, Cyclone)

**Status (2026-08-27).** W0–W3, W5 and **W6** LANDED — the board boots,
schedules, and delivers CycloneDDS pub/sub on Cortex-R52 **between two QEMU
guests over the emulated LAN9118**, which no board in this tree could do
before. W4 partial (fixture row registered; the runtime matrix cell is not
written). Filed from the ASI reference-consumer side (its view
lives in `docs/roadmap/phase-6-emulated-r52-lane.md` in
`NEWSLabNTU/autoware-safety-island`). Sibling to phase-372, which built the
S32Z270 bundle that has never RUN.

**Evidence (2026-08-26):**

```
Network ready
Published: 0
[INFO] nros: cpp_talker logging seq=0
Received: 0
...
```
plus 11 792 timer IRQs delivered in 12 s of wall clock at
`configTICK_RATE_HZ = 1000`, and the CPU observed in `sys32` inside
`vApplicationIdleHook` — i.e. the scheduler is running tasks, not spinning in a
fault handler.

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

* **W1 — bundle skeleton. DONE.** `packages/boards/nros-board-mps3-an536-freertos/`:
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
* **W2 — EL2→EL1, GICv3, tick, scheduler. DONE.** `c/board_an536.c` + startup asm:
  vector table, EL2→EL1 drop, per-mode stacks, `VBAR`, MPU disabled to start,
  GICv3 init (dist/redist/CPU-interface via the A32 `ICC_*` CP15 encodings),
  generic-timer tick on PPI 30, `vApplicationIRQHandler` (IAR → dispatch →
  EOI), UART console at `0xe7c00000`. Overlay
  `cmake/board/nano-ros-board-mps3-an536-freertos.cmake` mirroring the
  s32z270 one (env-provisioned `FREERTOS_DIR`/`FREERTOS_PORT`,
  `enable_language(ASM)` + `portASM.S`).
  *Acceptance: two FreeRTOS tasks alternate on the console and the tick
  count advances.*
* **W3 — networking. DONE** (lwIP reports `Network ready` over the emulated
  LAN9118; the netif needed no interrupt wiring, as predicted).
  Original scope: Strong `nros_board_register_netif` /
  `nros_board_poll_netif` over `lan9118-lwip` at base `0xe0300000`, static IP.
  *Acceptance: the host pings the guest.*
* **W4 — fixtures + CI. PARTIAL.** The `[[workspace_fixture]]` row
  `workspace-cpp-mps3-an536-freertos` is in `examples/fixtures.toml` and
  resolves to a coordinate (verified with `fixtures-manifest.py coords`), so
  the build lane and the freshness/coverage gates see the board. What is NOT
  written is the RUNTIME matrix cell: that needs a new `MP::` platform
  variant, and `FreertosMps2` has ~67 references across `platform.rs`,
  `matrix.rs`, `lane_scope.rs` and several test files, each with its own gate
  (`sched_dims_model_coverage`, `matrix_fixture_coverage` G1–G4). That is its
  own commit, not a tail end of this one.
  Original scope: `examples/fixtures.toml` witness row
  (`platform = "freertos"`, `NANO_ROS_BOARD = "mps3-an536-freertos"`, own
  `build_subdir`), `build-test-fixtures` lane membership, and a runtime cell
  so a tier actually runs it. *Acceptance: builds and boots from a clean
  checkout in CI.*
* **W5 — Cyclone entities. DONE, and past the stated bar.** The acceptance
  asked only for participant + writers/readers, matching MPS2. The image
  actually DELIVERS: `Published: N` / `Received: N` pairs, continuously. So
  CycloneDDS pub/sub works end to end on Cortex-R52 FreeRTOS.
* **W6 — Cyclone DELIVERY out of the guest. DONE (2026-08-27).** Two guests
  on a shared virtual LAN (`-net nic -net socket,mcast=<group>`, the second
  image rebuilt with `NROS_ENTRY_IP_LAST=11` so IP and MAC differ) exchange
  samples both ways:

  ```
  A: published=37  received=69
  B: published=32  received=64
  ```

  Each node receives its own samples PLUS the peer's — every `Received: N`
  value appears exactly twice — which is cross-node DDS delivery out of a
  QEMU FreeRTOS guest, the thing phase-370's stretch goal never claimed.

  **The stall reported on 2026-08-26 was not a defect.** It looked like one:
  one of the two nodes would boot, print `Network ready`, and never publish,
  while the other ran normally; the victim was alive (sampled in `sys32`
  inside `vApplicationIdleHook`) and fine when run alone. Two observations
  broke it open — the victim SWAPPED between runs (so it was not "the second
  node"), and once a node stalled while its partner had failed to start at
  all, so it was alone on the group. The cause was leftover QEMU instances
  from earlier runs still joined to the SAME multicast group, replaying the
  same image and therefore the same IP and MAC: duplicate participants,
  ghost peers, selective delivery. On a fresh group with no strays, both
  nodes work every time.

  This is the ghost-instance class the ASI consumer already records twice
  (its phase-4 notes: an orphaned FVP survived pid-file kills and ran
  concurrently on the same tap IP/MAC; and issue 0746, where `ros2 topic hz`
  was aggregating three stale island processes). It now has a third
  instance, in this repo, and the rule generalises: **before believing any
  multi-node result on a shared QEMU LAN, prove the participant count** —
  `pgrep -a qemu-system-arm`, and use a distinct multicast group per
  experiment.

  **Guest-to-HOST is RESOLVED** (2026-08-27). A host ROS 2 stack and the
  emulated Cortex-R52 now exchange data in both directions over `tap1`:

  * host `ros2 topic echo /chatter` receives **39** of the guest's samples;
  * the guest logs the host's published `12345` **20** times;
  * `ping 192.0.3.10` from the host: 0% loss, neighbour `REACHABLE`.

  The cause was NOT nano-ros. QEMU's net hub, when it holds only the board NIC
  and the tap, never delivers host-to-guest frames — see **issue 0830**. The
  fix is one extra flag on the QEMU line:

  ```
  -net nic -net tap,ifname=tap1,script=no,downscript=no \
  -netdev hubport,id=h0,hubid=0
  ```

  Measured back to back, twice each: two ports gives 0 frames at the driver
  and 100% ping loss; three ports gives 19-20 frames and 0% loss.

  **Two hypotheses recorded here earlier were WRONG, and are corrected rather
  than quietly dropped:**

  * *"It is an endpoint-matching problem; the lead is `rt/` topic mangling."*
    Disproved. The same host `ros2 topic echo /chatter` now matches the
    guest's writer with no naming change on either side. The guest-to-guest
    result never implied a naming convention mismatch; it only meant both ends
    agreed, which a working ROS peer now also does.
  * *"The host cannot transmit on tap1."* This came from watching
    `/sys/class/net/tap1/statistics/tx_packets`, which stayed frozen while the
    interface was demonstrably sending. Attaching to the tap directly
    (`TUNSETIFF`, in place of QEMU) showed the ARP requests and RTPS multicast
    leaving normally. On a tun/tap device that counter is not evidence of what
    reached the wire — read the frames.

  What actually localised it was instrumenting `lan9118_lwip_poll()` to print
  every frame the driver received, then running the SAME binary against a
  `-net socket,mcast` backend as a control: non-zero there, zero on tap. That
  put the loss between QEMU's tap reader and the NIC model, which is the only
  place neither the host nor our driver could be blamed for.

  Also landed here: the driver now clears `MAC_CR.BCAST` explicitly. The bit is
  inverted (SET disables broadcast), so inheriting an unknown reset value could
  kill ARP while leaving multicast working — exactly this symptom's shape. It
  is hardening, not the fix; clearing it changed nothing on QEMU, and neither
  did full promiscuous mode.

## Correction carried in from the consumer's scoping

The ASI scope first called the Cyclone milestone "a port, not a bring-up", on
the strength of phase-370's status line. Read closely, that phase claims the
MPS2 cell *"builds, boots, and creates writers and readers"* — while its own
stretch goal, *"one QEMU MPS2 cyclonedds cell boots and DELIVERS locally"*, is
not claimed as met. Cross-node DDS delivery out of a QEMU FreeRTOS guest was
therefore unproven in this repo, which is why W5 and W6 are separate items with
separate acceptance. **W6 has since proven it** (2026-08-27, above) — the gap
was real and is now closed, on AN536 rather than MPS2. Worth fixing phase-370's
summary line too, since that is what the misreading came from.

## Defects found on the way (2026-08-26)

Four, and only one of them is about this board. Each was a silent failure that
looked like something else:

1. **`[arch.cortex-r52]` was unreachable.** The profile landed with phase-372
   W1 but was never added to the platform manifest's `arch = [..]`, and the
   resolver walks that list — so an `armv8r-none-eabihf` cargo build panicked
   "no `[arch.*]` profile … admits TARGET", naming only m3 and m7. The S32Z270
   board never noticed because its CMake lane passes `FREERTOS_CFLAGS`
   explicitly (`nros_armv8r_cflags_env`), which short-circuits the lookup.
2. **The family's semihosting console is Thumb-only.** `bkpt #0xAB` is the T32
   encoding; in ARM state QEMU does not recognise it as a semihosting call and
   takes a real abort, so the image spins in the abort vector with NO OUTPUT —
   indistinguishable from a dead image. Now selects `svc #0x123456` for ARM
   state via `__thumb__`. An R-profile board could never have had a console
   before this.
3. **The FPU must be on before the kernel runs.** `pxPortInitialiseStack`
   zeroes FP context with `vmov.i32 d16, #0`; with CPACR/FPEXC unset that is an
   UNDEFINED INSTRUCTION. `HCPTR` must also be cleared at EL2 *before* the drop,
   or the EL1 access traps upward instead.
4. **`activate.sh` exports `FREERTOS_PORT=GCC/ARM_CM3` repo-wide**, so sourcing
   it and configuring an R52 board silently selects a port whose context switch
   is written in `msr basepri`. The assembler rejects it hundreds of lines into
   the kernel build, where the cause is unrecognisable. The overlay now
   `FATAL_ERROR`s on an `ARM_CM*` port and names the cause. **The S32Z270
   overlay still walks into this**, and #1 above means its cargo lane cannot
   work either — both worth a follow-up on that board.

Also worth recording for whoever writes the S32Z270 hardware bring-up: items 2
and 3 are exactly the "consumer early-init" that bundle defers, and this board
now has a working implementation of both to copy.

## Acceptance (phase)

1. `nros-board-mps3-an536-freertos` boots on QEMU, schedules tasks, ticks.
2. lwIP is up over the emulated `lan9118`; the host can ping it.
3. A fixture builds and boots the board in CI from a clean checkout.
4. Cyclone creates entities (W5) and delivers between guests (W6). A HOST
   peer additionally needs tap (root) — see W6.
5. The consumer lane (ASI `freertos-an536`) builds and boots its controller
   image against this bundle — consumer-side acceptance lives in ASI phase-6.

## Non-goals

* SMP. an536 is dual-core; run core 0 only.
* MPU/PMSAv8 region design beyond "enough to run" — deferrable, and the S32Z2
  memory map is the one that will actually matter.
* Replacing the S32Z270 bundle. This board proves the ARMv8-R SOFTWARE stack;
  NETC, PBcfg, the licensed `GCC/ARM_CR52_GIC` port and flash/boot stay
  hardware-gated there.
