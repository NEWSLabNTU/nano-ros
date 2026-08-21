---
id: 736
title: "`realtime_tiers` nuttx-arm/rust: the 10 ms tier's TIMER fires 7 times in
  1000 spins — the tier is scheduled, its clock is not advancing with it"
status: open
type: bug
area: core, platform, testing
related: [phase-281, issue-0636, issue-0623]
---

## Symptom

```
cargo nextest run -p nros-tests --test realtime_tiers_e2e --retries 0

nuttx-arm/rust: high-tier /ctrl counter 2 is not >= 3x the low-tier /telem
counter 21 — the 10 ms tier is not outrunning the 100 ms tier
```

The FAST tier delivers fewer messages than the SLOW one, inverted ~70x.
Deterministic.

## The measurement that reframes it

Run the fixture by hand for 30 s (`qemu-system-arm -M virt -cpu cortex-a7
-icount shift=auto -kernel <nuttx_entry> -netdev user,...`, with `just zenohd
tcp/0.0.0.0:8291` for the baked locator) and the tier reports on itself:

```
nros: tier `high` alive — 1000 spin(s), 7 timer(s) fired, 0 sub callback(s), 0 error(s)
```

**1000 spins, 7 timer fires.** The tier is being scheduled — a thousand times.
Its spin period is 1000 us and its timer period is 10 ms, so the timer is due
every ~10 spins and should have fired ~100 times. It fired 7, i.e. the timer's
clock advanced ~70 ms across 1000 spins that asked for 1 s.

So this is NOT a scheduling defect, and everything the assertion's wording
suggests ("the 10 ms tier is not outrunning the 100 ms tier") points the reader
at the wrong layer. The tier runs. Its sense of time does not keep up with it.

The `low` tier is the control: 10 ms spin, 100 ms timer — the same 10-spins-per-
fire ratio — and it delivers at roughly its declared rate. Whatever this is, it
bites the 1 ms spin and not the 10 ms one.

## Ruled out, with the measurement for each

* **SCHED_SPORADIC.** `[tiers.high.nuttx] budget_us=5000 period_us=10000`
  engages it, and `sched_ss_max_repl` was hardcoded to 1 — genuinely wrong (see
  below). Raising it to `CONFIG_SCHED_SPORADIC_MAXREPL` left the symptom bit for
  bit unchanged (ctrl 2, telem 21). Compiling the sporadic call out entirely
  gave ctrl 5 / telem 13 — moved, still inverted. Not the cause.
* **#636 / the boot-tier choice.** `boot_tier_index` did move `high` from the
  boot thread to the spawned path, which is why this became reachable, but the
  spawned tier gets its priority (`tier priority set tier=high prio=110`) and
  runs 1000 spins. It is scheduled.
* **phase-359 W10's clock ruling.** NuttX has had no `std` since W7, so both
  `Clock::now()`'s `no_std` body and `default_clock_us_fn`'s
  `not(rmw-cffi)` arm are the same before and after W10. Checked against
  `121b555c9^`.
* **A museum binary.** This issue's first draft claimed `lane=native` does not
  rebuild the row so every run used a stale fixture. Wrong: the row produced
  runtime output, so the freshness gate passed it. It has since been rebuilt
  with `just nuttx build-fixtures` and behaves identically.
* **"Fails solo, passes in the sweep."** Also this issue's first draft, also
  wrong, and worth recording because it invented a mystery. `just ci` is tier 1;
  `nros_tests::lane_scope::admits` puts the nuttx rows OUT OF LANE. Measured:
  `NROS_TEST_SCOPE=native` -> "4 row(s) ran, 12 out of lane". The 47 s vs 21.6 s
  was 16 rows against 4, not a load effect.

## Where to look next

The tier's spin loop and the clock its timer compares against are the two
suspects, and the 1 ms-vs-10 ms split is the discriminator. Concretely: does
`spin_once` return early without waiting its declared period (so 1000 spins
really did take only ~70 ms of guest time), or does it wait correctly while the
clock under-reports? The tier's own counters can tell these apart — a spin count
alongside the clock delta at the alive report would settle it in one run, and
that instrumentation does not exist yet.

Related but separate: the executor's contract monitor prints
`timer-overrun-runtime timer measured=8 declared=0` continuously. `measured=8`
means a publish costs ~8 ms on this emulated target while the deployment
declared no runtime at all, so the declaration is empty and the monitor cannot
do anything but complain. That was this issue's ORIGINAL title, before the
counters showed it to be a passenger rather than the driver.

## A second, different failure this uncovered

`nuttx-riscv/rust` also fails, and not the same way: `/ctrl` counter **0**, no
`tier priority set tier=high` line at all, no FIRST dispatch, and `sporadic
budget FAILED tier=high rc=22` (EINVAL) where the arm build succeeds. It was
invisible because its fixture had never been built — `realtime_tiers` reported
"16 ran, 10 skipped" until `just nuttx build-fixtures` brought it down to 5.

## Reproduce

```
just nuttx build-fixtures
cargo nextest run -p nros-tests --test realtime_tiers_e2e --retries 0
```

## Measured 2026-08-21 — the clock is not BEHIND, it is 6.5x AHEAD

The issue asks for "a spin count alongside the clock delta at the alive report
… that instrumentation does not exist yet". It exists now (the NuttX tier loop
reads `nros_platform_clock_ns` — the same monotonic source the timers compare
against, deliberately, since a second healthier clock would only prove two
clocks disagree) and it answers the question the other way round:

```
tier `low`  100 spin(s),   54 timer(s) fired, clock  6551000 us vs asked 1000000 us
tier `low`  200 spin(s),  125 timer(s) fired, clock 15460000 us vs asked 2000000 us
tier `low`  300 spin(s),  190 timer(s) fired, clock 24558000 us vs asked 3000000 us
tier `high` 1000 spin(s),   5 timer(s) fired, clock 25924000 us vs asked 1000000 us
tier `low`  400 spin(s),  246 timer(s) fired, clock 31448000 us vs asked 4000000 us
```

**"The tier's sense of time does not keep up with it" is backwards.** The clock
runs 6.5–7.9x AHEAD of the time the spins asked for — unsurprising under
`-icount` emulation, where a 1 ms spin cannot complete in 1 ms of guest time.
Neither candidate the issue named is what happened: `spin_once` is not returning
early, and the clock is not under-reporting.

### What the numbers actually say

Comparing the two tiers at the same point in guest time:

| | spins | clock | timer fires | fires the clock implies | ratio |
| --- | --- | --- | --- | --- | --- |
| `low` (10 ms spin, 100 ms timer) | 300 | 24.6 s | 190 | ~245 | **78 %** |
| `high` (1 ms spin, 10 ms timer) | 1000 | 25.9 s | 5 | ~2590 | **0.2 %** |

`low` is roughly keeping up. `high` is not, by three orders of magnitude — and
crucially **not** by a one-fire-per-`spin_once` cap either: that cap would give
`high` ~1000 fires, and it produced 5. `low` meanwhile manages 0.63 fires per
spin, so more than one fire per spin is clearly reachable.

Each `high` spin also consumes ~26 ms of clock for a declared 1 ms — so by
elapsed time the timer is ~26 periods overdue on EVERY spin, and still does not
fire.

### One hypothesis eliminated before it is proposed

Tick granularity is the obvious suspect and it is **not** a 10 ms-tick problem:
the built image has `CONFIG_USEC_PER_TICK=1000` with `CONFIG_SCHED_TICKLESS`
unset, so a 1 ms spin period is exactly one tick, not a sub-tick round-to-zero.
Recorded because it is where a reader would go first.

### Where that leaves it

The defect is in when the executor decides a timer is DUE, on a tier whose spin
period is 1 ms — not in scheduling, not in the clock, and not in the wait. The
next measurement is inside `spin_once_counted`: what it computes as the next due
time and what it compares against, for a 1 ms period versus a 10 ms one.

### Reproducing the numbers

The e2e window is too short to reach a heartbeat — the fast tier needs 1000
spins and gets ~39 per second of guest clock. Use the manual run, which now
works in the ROS distrobox (it needs both QEMU and a router, and until this
issue neither host had both — the box lacked `qemu`, and a ROS-less host has no
`rmw_zenohd`):

```
just zenohd tcp/0.0.0.0:8291 &
qemu-system-arm -M virt -cpu cortex-a7 -nographic -icount shift=auto \
  -kernel examples/workspaces/realtime-rust/target-fixtures/nuttx/armv7a-nuttx-eabihf/nros-minsizerel/nuttx_entry \
  -netdev user,id=net0 -device virtio-net-device,netdev=net0
```

