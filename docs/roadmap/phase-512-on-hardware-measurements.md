# Phase 512 — Work that needs a real board

**Status (2026-10-26). Batch A done; H1-H5 blocked on hardware.** One S32K344
board (MR-CANHUBK344) is on a bench. Batch A ("Batches", below) closed H6-H9 on
it: H6 from existing board runs, and H7, H8 and H9 in one bench session on
2026-10-26. Issues 1799 and 0902 are archived. The session filed 2077 and
2078. H1-H5 still wait on hardware this bench does not have.

Each item is a measurement whose code has landed and whose host or emulator
evidence is recorded in the phase it came from. Only the silicon number is
missing. The items are grouped by the board that runs them, so that one bench
session closes every item that board can close.

**Prior:** [phase-436](archived/phase-436-poll-wake-revision-deadline-driven-executor.md)
(the deadline-driven executor, RFC-0106).

**Gathered 2026-10-24:** [phase-162](archived/phase-162-rt-scheduling-harness.md) 162.C
(H4), [phase-418](phase-418-orin-safety-island-and-spe.md) 418.5 and 418.7
(H5), the board leg of [phase-474](phase-474-safety-island-board-findings.md)
T4 (H6), [issue 1799](../issues/archived/1799-boot-report-arena-exhaustion-on-mr-canhubk344.md)
(H7), and [issue 0902](../issues/archived/0902-action-goal-completion-is-variable.md)
(H8, moved 2026-10-24). Each source phase keeps a pointer where the item was. Board reruns that
belong to the safety island's own phase (474 I6, I8, I9 and 446-W7's board
leg are "the island's W4") stay in phase-474: they are blocked on a downstream
project as much as on a board.

---

## Batches

Each item needs one board. Grouped by board, the list is five bench sessions,
not eight separate runs:

| Batch | Board | Items | State |
| --- | --- | --- | --- |
| A | MR-CANHUBK344 (S32K344), SWD probe, serial link to a router | H6, H7, H8, H9 | done 2026-10-26 |
| B | S32Z270 silicon | H1 | no board |
| C | a bare-metal Cortex-M board: MPS2-AN385 or STM32F4-Discovery | H2, H4 | no board; H2 also needs a board crate that registers smoltcp's deadline source |
| D | a host with root, tap-fed QEMU | H3 | no runner with root |
| E | Jetson AGX Orin devkit | H5 | no devkit |

Batch A runs in this order, one flash per image: H7's two console-less talker
images (fatal, then the non-fatal control), then H8's action images over
serial (N runs, `zpico_reply_slot_declines` read throughout), then H9's Rust
level image. H6 needed no new run.

Batch A's session (2026-10-26, nano-ros 9f2d7b4bb3): the board ran behind a
stock `rmw_zenohd` listening on the board's serial link (921,600 baud) and on
TCP for host peers. The board's transport was lpuart2; its console is on the
unwired lpuart0. Every image needed one local, uncommitted link fix, the
`.nros_boot_config` placement (issue 2077). The H9 image also needed a
wait-network stub (issue 2078) and its tier stack pool in DTCM (issue 2084,
resolved 2026-10-27). Logs, dumps, configs and the local diffs are
kept with the island's run archive for 2026-10-26 (`board-batch/`).

## Why a separate phase

The code these items measure is on `main`. Its evidence so far comes from
emulators: the deadline sources from the QEMU mps2-an385 run in phase-436
Path B, and the park probe from ASI's FVP image in phase-436 A3. An emulator's
clock does not advance like silicon's. Phase-436 A3 records that the FVP
counter keeps running during WFI at a rate unrelated to elapsed time. So an
absolute latency, or a timing-dependent win, has to be re-taken on a board.

Spread across their source phases, these items read as open work nobody can
pick up. Gathered here, they form one list to work through when a board is
attached.

## Work items

### H1 — absolute park latency on S32Z silicon

From phase-436 A3.

A3's loaded run on ASI's FVP image stays in phase-436. On FVP it can only show
that the probe measures the right thing in the right place: the probe and the
CTF trace share CNTVCT, which the FVP advances during WFI at a rate unrelated
to elapsed time. An absolute wake latency needs silicon (S32Z270), or a short,
low-idle capture under the 4.295 s wrap of the 32-bit CTF timestamp.

**Done when** the `rt-probe:` wake-latency figures from ASI's controller, run
on S32Z silicon with the same configuration as the FVP run, are recorded here
beside the FVP numbers.

### H2 — a platform deadline source winning on a real bare-metal board

From phase-436 Path B.

Path B's target run was met on QEMU on 2026-10-16. With a live zenoh session
and a 50 ms budget, mps2-an385 printed `park bounded by Platform(0) at 27000 us`.
No run has been made on hardware. Today only the two QEMU boards,
`nros-board-mps2-an385` and `nros-board-esp32-qemu`, register smoltcp's
deadline source, so a silicon run first needs a bare-metal board crate that
registers it.

**Done when** a real board with a live session prints a `Platform(n)`-won park.
The report line fires on the first one, so the run answers within seconds.

### H3 — an 8-fragment RTPS burst on a free-running, tap-fed QEMU

From [phase-441](archived/phase-441-rmw-on-target-verification.md) W6, for
[issue 0917](../issues/0917-an536-fragmented-sample-never-syncs.md).

This is the one item here that is not silicon. It is a host setup no lane
provides. Phase-441 W6's fragmented leg passed in tree on 2026-10-20 on the
mps3-an536 FreeRTOS Cyclone listener: 725 of 751 samples of 10 576 bytes (eight
`DATA_FRAG`s each) arrived whole, in 5 of 5 runs, over slirp under
`-icount shift=auto`. The island's failure was on a different path: a QEMU
running free (no `-icount`) and fed unpaced frames from a root-provisioned
tap. That path needs root on the host, and no runner has it.

**Done when** the same listener, on a free-running QEMU behind a tap, either
keeps receiving whole 8-fragment samples for 45 s (then 0917 closes as not
reproduced) or drops them (then that run is 0917's first in-tree
reproduction, with the RX FIFO depth and the drain interval recorded).

### H4 — Cortex-M3 P99 wake latency on silicon

From phase-162 162.C. The items below carry 162.C's 2026-09-06 corrections
(`wake_alloc.rs` is gone, so the hooks are in `spin.rs`; the crate never had a
README). The bench once rotted into no build lane and did not compile
([0317](../issues/archived/0317-wake-latency-cortex-m3-bench-rotted-off-lane.md)); phase-313
resurrected it.

The `wake-latency-cortex-m3` bench (`packages/testing/nros-bench/`) builds in
the `freertos` fixture group and runs under QEMU, but QEMU's timing says
nothing about a silicon P99. The four items, unchanged in substance:

- [ ] **H4.1** (was 162.C.1) Procurement checklist in
      `docs/reference/hardware-procurement.md`: MPS2-AN385 or
      STM32F4-Discovery, OpenOCD / ST-Link / J-Link, a USB-UART adapter.
- [ ] **H4.2** (was 162.C.2) Programming guide for the bench on hardware: the
      capture command and the host-side parser invocation for its UART CSV.
- [ ] **H4.3** (was 162.C.3) Run it; assert P99 ≤ 100 µs across the three
      phase-141.D scenarios. Closes phase-141's spec-acceptance gate.
- [ ] **H4.4** (was 162.C.4) The 10× baseline: no-op
      `nros_rmw_runtime_wake_cb` AND `nros_rmw_runtime_wake_cb_from_isr` in
      `nros-node/src/executor/spin.rs`, measure, restore, and record the number
      in a new `wake-latency-cortex-m3/README.md`.

**Done when** H4.3 and H4.4 have numbers from the same board.

### H5 — the Orin SPE on a Jetson AGX Orin devkit

From phase-418 418.5 and 418.7. Provisioning (418.4), the board crate (418.6)
and the bridge decision (418.8) need no devkit and stay in phase-418.

- [ ] **H5.1** (was 418.5) A non-`aon_echo` IVC channel in the AGX Orin DTSI,
      and its Linux-side owner (a small out-of-tree kernel module with a char
      device, or reuse of the echo channel's driver), with the kernel rebuild
      and flash steps documented. **Acceptance:** a loopback between a
      userspace process on Jetson Linux and the stock `ivc-echo-task` on the
      SPE, with the frame format matching `nvidia_ivc_mock_wire_format.rs`.
- [ ] **H5.2** (was 418.7) A talker on the SPE reaching `ros2 topic echo` on
      Jetson Linux through the CCPLEX bridge and `rmw_zenoh_cpp`.
      **Acceptance:** an `interop::CELLS` row (no fixture row: the SPE side is
      flashed, not launched), and promoting `nros-board-orin-spe`'s
      `board-support.toml` row off tier 3 once a lane can assert it.

**Done when** H5.2's echo prints on the devkit.

### H6 — a deliberate overrun reported through D1's channel on the S32K344

From phase-474 T4. The host executor e2e and the native_sim fixture
(`violation_channel_e2e`) have landed. The QEMU leg stays in phase-474,
because it needs no board.

The board run of 2026-10-06 showed the overrun as timer-overrun and jitter
verdicts, not as `max-latency-runtime`. That was phase-474 I6, whose host half
has landed since (#1750). So this run is also the first board check of I6's
fix.

**Done when** a deliberate overrun on the MR-CANHUBK344 appears in the
violation ring read over SWD as a `max-latency-runtime` verdict.

**Met 2026-10-26.** The island's overrun acts (`RB_OVERRUN=300`; runs
`i2041-ov1` on nano-ros 8b95e90dd6, `i2065-ov1` on 5feeb32a7b) each stored
three `max-latency-runtime` verdicts in the SWD record, one per monitored
publisher of the overrunning timer tick, for example
`max-latency-runtime /mrm_handler/mrm_state measured=301 declared=149 (one dispatch, ms)`.
Evidence and the other board halves (phase-474 I6, I8, I9) are recorded in
[phase-474](phase-474-safety-island-board-findings.md), "The board halves,
2026-10-26".

### H7 — the boot report's arena-exhaustion record, dumped from silicon

[Issue 1799](../issues/archived/1799-boot-report-arena-exhaustion-on-mr-canhubk344.md),
homed here from phase-474. Every other step of the path runs in a lane: the
host decode tests, and the QEMU `mps2/an385` boot-report fixture that dumps
the record with `pmemsave`. Only the MR-CANHUBK344 dump is missing.

**Done when** the issue's board run decodes to the same verdict as the QEMU
lane, and the issue is archived.

**Done 2026-10-26.** The fatal image halted in `arch_system_halt` and decoded
to `stage 4 RegisteringEntities`, `ARENA EXHAUSTED: an allocation of 48 bytes
did not fit, short by 16 bytes`, `set NROS_EXECUTOR_ARENA_SIZE >= 48`. The
non-fatal control decoded the same way and ran on. Both decodes are in the
archived issue.

### H8 — action goal completion rate on the MR-CANHUBK344, over serial

[Issue 0902](../issues/archived/0902-action-goal-completion-is-variable.md), moved
here from phase-474 on 2026-10-24. On a bare-metal serial board inside a real
ROS 2 graph, goals were accepted and then completed in 20 % to 90 % of runs on
one build. The native lane now gates the completion path end to end
(`just linux test-action-completion`, 3/3), but issue 1332 showed that lane
cannot exercise the reply-slot path at all: a one-slot table refuses nothing
there. Only the board, inside a real graph, has the peer population that
showed the symptom.

**Done when** a freshly built image on the MR-CANHUBK344 (`mr_canhubk3/s32k344`),
over its direct serial link, a router and a live ROS 2 peer, has its goal
completion rate recorded over enough runs to separate it from the 20–90 %
spread, with `zpico_reply_slot_declines` read during the run. Non-zero puts
the failure back on the reply-slot path; zero with goals still failing rules
that path out.

**Done 2026-10-26: not reproduced.** 50 of 50 goals SUCCEEDED over five fresh
boots: 5 goals, a 100 s idle soak, then 5 goals per boot, with live
`rmw_zenoh_cpp` peers. `zpico_reply_slot_declines` read 1 per boot, on the
TRANSIENT_LOCAL status queryable only; the action queryables declined 0. Issue
0902 is archived as not reproduced. The image needed a 32 KiB main stack; at
16 KiB main faulted before registration (the 0852 signature).

### H9 -- the Rust runner's Zephyr level worker frame on silicon

From [issue 2065](../issues/archived/2065-zephyr-level-worker-frame-is-an-estimate.md)
(resolved by PR #2682). On the S32K344 the C runner's frame was measured at
13,312 B, and that is now `CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME`'s default.
The Rust runner's 51,200 B (`level_worker_frame_bytes`, Zephyr, Rust) comes from
QEMU mps2-an385 only: the `realtime-rust` `derived_bringup` fixture's setup
peak, 50,968 B in a debug build and 47,456 B in release, with a steady peak of
about 5.9 KB.

**Done when** the same fixture, built for `mr_canhubk3/s32k344`, has its
setup and steady peaks read on the board, and the Rust frame is confirmed or
corrected from them.

**Done 2026-10-26: 51,200 B confirmed.** On the board, read over SWD:

| profile | setup peak | steady peak (300 / 3,000 / 10,000 passes) | QEMU setup |
| --- | ---: | ---: | ---: |
| release | 46,868 B | 5,832 B | 47,456 B |
| debug (`CONFIG_DEBUG=y`) | 50,376 B | 5,968 B | 50,968 B |

Both are under the 51,200 B frame. The image needed its level stack in DTCM
(`MAX_TIERS=1`, 64 KiB) to fit. In the debug build, the boot tier's main
overflowed 32 KiB; at 56 KiB its high-water read 55,992 B. The release main
was not read.

## Issues homed here

| issue | why it belongs here |
| --- | --- |
| [#1799](../issues/archived/1799-boot-report-arena-exhaustion-on-mr-canhubk344.md) | H7: resolved 2026-10-26, archived. |
| [#0902](../issues/archived/0902-action-goal-completion-is-variable.md) | H8: not reproduced 2026-10-26, archived. |
| [#2077](../issues/archived/2077-nros-boot-config-orphan-lands-in-s32k344-ivt-header.md) | found by batch A: typed entries did not link on the S32K344 without a local section placement. Resolved 2026-10-26 (the module places the section in rodata), archived. |
| [#2078](../issues/archived/2078-zephyr-rust-serial-image-needs-wait-network.md) | found by batch A (H9): a serial-only Zephyr Rust image did not link. Resolved 2026-10-26 (the wait is defined without sockets), archived. |
| [#2084](../issues/archived/2084-zephyr-tier-stack-pool-ignores-stacks-in-dtcm.md) | found by batch A (H9): the serial Rust level image overflows RAM by 30,028 B on the S32K344; `CONFIG_NROS_ZEPHYR_STACKS_IN_DTCM` does not move the tier stack pool. Resolved 2026-10-27 (the tier stacks follow the knob through `CONFIG_NROS_ZEPHYR_TIER_STACKS_IN_DTCM`), archived. |

## Not in this phase

- Work that is blocked on a downstream project rather than on a board, such as
  ASI's port to the one node type before A3's loaded run. It stays in
  phase-436.
- Designing new hardware lanes or a hardware-in-the-loop harness. Phase-162
  owns the harness.
