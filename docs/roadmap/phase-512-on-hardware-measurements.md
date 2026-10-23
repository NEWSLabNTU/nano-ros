# Phase 512 — Measurements that need a real board

**Status (2026-10-23). Not started — blocked on hardware.** No board is on this
bench, and no CI lane provides one. Each item below is a measurement whose code
has landed and whose host or emulator evidence is recorded in the phase it came
from. Only the silicon number is missing. Each item moved here so that the
phase it came from lists only work someone can do today. Unblock an item when
its board is attached; each one is independent of the others.

**Prior:** [phase-436](phase-436-poll-wake-revision-deadline-driven-executor.md)
(the deadline-driven executor, RFC-0106).

**Related, not moved:** [phase-162](phase-162-rt-scheduling-harness.md) 162.C
(Cortex-M3 P99 wake latency on silicon), and the board runs in
[phase-474](phase-474-safety-island-board-findings.md) (T4's native_sim, QEMU
and board runs). Both phases own more than their hardware runs, so their board
items stay where their context is.

---

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

## Not in this phase

- Work that is blocked on a downstream project rather than on a board, such as
  ASI's port to the one node type before A3's loaded run. It stays in
  phase-436.
- Designing new hardware lanes or a hardware-in-the-loop harness. Phase-162
  owns the harness.
