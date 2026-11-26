---
id: 2971
title: "An ordinary violation-cpp tick costs 37..65 ms of guest time on mps2/an385 under icount, and nobody knows whether that is the emulator or nano-ros"
status: resolved
resolved: 2026-11-26
resolved_in: "the change that adds this section"
type: bug
severity: low
area: [testing, zephyr, qemu, monitors, zenoh]
related: [issue-2273, issue-2508, issue-2976, phase-544, phase-474]
found: 2026-11-25
---

**Resolved: the emulator.** Under a fixed `-icount` an ordinary tick costs
0.79 ms of guest time at current main and 0.74 ms at bf9fccba83. The 14..65 ms
readings are QEMU's adaptive `shift=auto` changing how much guest time an
instruction is worth. See [Resolution](#resolution).

**Home: [phase-544](../../roadmap/archived/phase-544-harness-run-plan-followups.md)** (the QEMU leg, issue 2273).

## What

`examples/workspaces/violation-cpp`'s handler does, on an ordinary 10 Hz tick, three
zenoh publishes and one `printf`. `max-latency-runtime` times callback entry to exit,
so the dispatch minus any busy-wait is the cost of that work. On mps2/an385 under the
board's `-icount shift=auto`, measured in guest time:

| date | main | how measured | ordinary dispatch |
| --- | --- | --- | --- |
| 2026-11-09 | bf9fccba83 | tick 6 (150 ms busy-wait) read 164..177, issue 2273 | 14..27 ms |
| 2026-11-13 | bf9fccba83, rebuilt | same tick read 189, 215, 189, issue 2508 table | 39, 65, 39 ms |
| 2026-11-13 | 121a6a99c6 + 110 ms overrun | tick 6 read 147 | 37 ms |
| 2026-11-25 | cffdca5c9b, tier 2 (four lanes at once) | tick 6 read 152; tick 5 (no busy-wait) read 63; tick 10 read 61 | 42, 63, 61 ms |

The same commit (bf9fccba83) read 14..27 ms on 11-09 and 39..65 ms when rebuilt on
11-13, which already argues against a per-commit regression: issue 2508's bisect found
no commit that owns the change. But the 11-09 numbers were never re-measured on a fresh
build, so the series cannot rule one out either.

Why it matters: the contract declares 50 ms on that path, so the board's ordinary
dispatch straddles its own bound. The test now identifies the provoked overrun instead
of counting verdicts (`check_provoked_overrun` in `violation_channel_e2e.rs`), which
holds only while an ordinary dispatch stays under the overrun's 110 ms. If the cost
is a real per-publish cost growing in nano-ros, it is a product bug that every
Cortex-M image pays, not a test margin.

## How to tell the two apart

`shift=auto` lets guest time follow host time while the guest waits, so host load and
SLIRP latency leak into what the guest measures. A fixed shift removes that:

1. Run the same `zephyr_mps2` image under `-icount shift=N,sleep=off` (N fixed, e.g. 4)
   instead of `shift=auto`, solo, 10 runs. Record the per-tick dispatch (temporarily
   lower the declared 50 ms, or log the dispatch time, so every tick prints a value).
2. Repeat at bf9fccba83 and at current main, both built fresh.
   - Same distribution at both ends under a fixed shift ⇒ the drift is emulation and
     host load. Close as such.
   - Higher at main under a fixed shift ⇒ a real per-dispatch cost grew. Bisect with
     the fixed shift, which makes each step deterministic.
3. Independently, split the cost: time each `publish` and the `printf` inside the
   handler, to say which of the three publishes (or the console) owns it.

## Follow-up option, not taken for the release

Make the 50 ms declaration true on that board instead of identifying the provoked
verdict. Today it needs a second bringup: the contract is one
`<stem>.contract.yaml` beside the launch file, with no per-image key in `system.toml`.
A per-image contract key would let the QEMU image declare its own budget, but a budget
derived from a peak that moves 14 → 65 ms is only stable once this issue says why it
moves.

## Resolution

Measured 2026-11-26 on the violation-cpp `zephyr_mps2` image, built fresh
at bf9fccba83 and at main (d92c369a14). `on_timer` was instrumented with
`k_cycle_get_32()` around each publish and the console `printf`. That
instrumentation was scratch and is not committed. Each image ran 10 times, solo,
under `-icount shift=5,sleep=off`, with its own router. The network works with
`sleep=off`. "Ordinary" ticks are 2..15, minus 3 (arming) and 6 (the overrun):
120 per side. Median per tick, in µs, with the range in brackets:

| | `/t4/state` | `/t4/answer` | `/t4/avail` | `printf` | whole dispatch |
| --- | --- | --- | --- | --- | --- |
| bf9fccba83 | 148 | 148 | 405 (401..406) | 33 | 735 (731..738) |
| main d92c369a14 | 159 | 160 (160..223) | 431 (427..432) | 33 | 786 (781..848) |

**The 14 → 65 ms series is the emulator, not a per-dispatch cost.** A
fixed shift makes both ends deterministic and both under 1 ms. That is
60x under the 50 ms contract and three orders of magnitude under the readings
in the table above. One `shift=auto` run of main's image shows the mechanism
directly. The same code's cost moves in exact powers of two from tick to
tick: the `printf` read 63, 127, 511 and 1023 µs against 31 at `shift=5`,
which is QEMU's adaptive shift stepping between 6 and 10. In that one run,
ordinary dispatches read 1.6 to 38.7 ms. QEMU raises the shift when the host
emulates slower than real time. Under `shift=auto` a guest-clock duration
therefore measures the host's load as much as the guest's work, and the
same commit could read 14..27 ms on one day and 39..65 ms on another.

**What follows for the tests.** A cell that asserts a bound on GUEST time
must boot at the fixed `MPS2_AN385_CORE_ICOUNT`
(`start_mps2_an385_networked_measured`), never at the board's
`shift=auto`. The same change switches the cells that did not. At a fixed
shift the guest clock is the instruction count of a busy guest, so the
bound is about the code.

**What is real: +51 µs per dispatch, about 7 %.** It is deterministic
under the fixed shift. About +11 µs on each of `/t4/state` and `/t4/answer`, and
+26 on `/t4/avail`, the publish the handler also takes itself (local
delivery). The `printf` is unchanged. Filed separately as
[issue 2976](../2976-local-delivery-publish-cost-grew.md). It is not this issue: it is
0.05 ms against a 30..50 ms drift.

How to reproduce: build the image, start `rmw_zenohd` on `tcp/0.0.0.0:10885`,
then
`qemu-system-arm -cpu cortex-m3 -machine mps2-an385 -nographic -icount shift=5,sleep=off -semihosting-config enable=on,target=native -nic user,model=lan9118 -kernel <zephyr.elf>`.
The busy-wait tick (6) reads 110,845 µs at main and 150,795 at bf9fccba83:
the overrun itself moved from 150 to 110 ms in between.

