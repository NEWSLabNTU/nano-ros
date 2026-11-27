---
id: 2965
title: "A spawned level parked in whole milliseconds, so a 1 kHz level released every 1.3 ms"
status: resolved
type: bug
severity: high
area: [executor, levels, real-time]
related: [phase-162, phase-509, issue-1242, phase-590]
found: 2026-11-25
resolved: 2026-11-27
resolved_in: "fix(executor, issue 2965): a spawned level parks at the port's finest park, not whole milliseconds (B33)"
---

## What happened

`rt_sched_levels_e2e::s1_critical_loop_meets_every_deadline_on_its_own_level`
failed on host-tests run 45533251762 (job 138933547464, a GitHub-hosted
ubuntu-22.04 runner, at `939ac840b2`):

```text
S1 reported 45 one-second windows in a 60 s run: the critical loop did not keep its rate
(AttitudeRun { windows: 45, ticks: 45000, misses: 0, worst_gap_us: 1520 })
```

The job's own console says this is not a start delay. The attitude node prints
one window per 1000 releases, and the image's log timestamps put those windows
**1.300 s apart**, every one of them (1964.877, 1966.177, 1967.478, ...). The
telemetry level (10 Hz) printed 13 ticks per window and all 600 of its ticks in
60 s, so the run lasted the full minute. 1000 releases in 1.3 s means one
release every 1.3 ms: the 1 ms period plus the attitude callback's 300 us of
work (`kWorkNs` in `attitude_pkg/src/Attitude.cpp`).

The same console has 600 lines of
`contract violation: timer-overrun-runtime timer measured=23..25 declared=0`,
one per 100 ms. That is the `Skip` overrun counter of the attitude timer:
100 periods per 100 ms, of which about 77 were dispatched (one per 1.3 ms) and
23 dropped. `misses` stayed 0 because no gap between two callback starts
exceeded 2 ms (period plus deadline): the loop was slow, not late.

## Cause

A spawned level is a band worker over the shared executor. It parks on its own
wake object until its band's next timer is due (`band_prelude` in
`executor/spin.rs`). The waiter every level runner gets,
`BandWaiter::platform()` (`executor/node_wake.rs`), waited through
`nros_platform_wake_wait_ms` and rounded the bound UP to whole milliseconds:

```rust
let ms = us.div_ceil(1000).min(u64::from(u32::MAX)) as u32;
```

After a release, the attitude level's next deadline is 1000 us minus the 300 us
the callback took, so the park bound is 700 us. Rounded up, the park was
1000 us, and the next release came 1.3 ms after the last. Each release then
owed another 300 us, and every third or fourth one found two periods elapsed,
which `Skip` dropped and counted.

The port already has the finer park: `nros_platform_wake_park_until_us`, at
`nros_platform_wake_park_granularity_us` (1 us on Linux hrtimers, a tick on
an RTOS). Every OWNER executor uses it (`set_park_primitive`, phase-436 C2).
The band waiter was the one park that still used the millisecond ABI.

**Why the job is red only sometimes (4 of the last 15 runs).** A band waiter
is also woken by an arrival that the executor cannot attribute to a band
(`an_attributed_arrival_wakes_its_band_and_an_unattributed_one_wakes_all`). On
a host whose session traffic wakes the worker before the rounded park ends, the
level keeps its rate by accident. A run with little such traffic gets the
millisecond park every time, as this one did.

## Fix

`NodeWake::wait_us` parks through `nros_platform_wake_park_until_us`, with the
bound rounded up only to the port's own granularity (`band_park`). A port that
offers no finer park (granularity 0, the `PlatformThreading` default) or
answers -1 keeps the millisecond wait. `BandWaiter::platform()` waits through
it. That waiter is the only one a level runner is given: `nros-cpp`'s
`nros_cpp_level_open` (every C/C++ level runner, on every port) and
`nros::levels`. The other `BandWaiter::new` sites are the RTIC run-to-completion
levels, which never wait, and unit tests.

## Verified (host 118, Linux 6.8, the test's docker launcher, `taskset -c 0`)

The `linux_s1` image, run for 30 s, before and after the fix:

| Image | Windows in 30 s | Timer overruns (`timer-overrun-runtime`, summed) | Lines |
| --- | --- | --- | --- |
| before | 29 | 145 | 118 |
| after | 30 | 1 | 1 |

On 118 the unattributed wakes hide most of the rounding, so the window count
moved by one. The overrun count shows the mechanism: 145 dropped releases
before, 1 after.

Unit tests: `executor::node_wake::band_park_tests` (a 1 us port parks for the
bound itself; a tick port rounds up to its tick; a port with no microsecond
park keeps the millisecond, the negative control).

## Reopened: S1's generic-kernel verdict measured the host (2026-11-25)

With the fix in, rt-sched S1 went red again at `cffdca5c9b` (host-tests run
45596611184, job 139144253612, a GitHub-hosted ubuntu-22.04 VM). The loop
kept its rate (59 windows, 59,000 releases), and the verdict failed on its
generic-kernel bound: 55 misses in 59,000 against the control's 94 in 8,000
is 12.6 times below, and the bound wanted 50.

**The misses were the host's, and the console says so twice.**

- They came in bursts (windows 24-27, 34-35, 44-46, 51-54) and 33 of 59
  windows had none. Telemetry releases ten times in every window, so a miss it
  caused would land in every window. The control, where it does run in front,
  misses in all of them, about one miss per release (94 in 80).
- The worst S1 gap was 7,239 us. The control's worst, which IS telemetry's
  5 ms burn in front of the loop, was 5,409 us. No telemetry release can cause
  a gap longer than that.
- And all 600 telemetry releases in the S1 run show the loop preempting them:
  wall time minus CPU time 2,149-3,139 us, about seven 300 us loop callbacks
  each. In the control that difference is about zero.

The fix made this visible: the loop releases on exactly 1 ms now, so any host
stall longer than about 1 ms is a miss. Before, it released every 1.3 ms, and
that was the defect the window count caught.

**Why the bound was wrong.** It compared S1's miss RATE with the control's
over a fixed divisor of 50, fitted on a quiet 6.8 kernel ("0-5 of 59,000").
The control's rate is set by telemetry and S1's by the host's stall floor, so
their ratio measured the host. Lowering the divisor would fit it to one VM.

**The verdict now reads the claim directly.** The claim the levels own is that
telemetry no longer runs in front of the critical loop. `load_pkg` now prints
`max_run_us` per release, the longest stretch it held the CPU without a step
off it. Below a loop that releases every 1 ms, that stretch stays under the
loop's miss gap (period plus deadline, 2 ms). A stretch past it is a moment
the loop was overdue while telemetry held the CPU. A host stall cannot make
one, because it takes the CPU from telemetry too, and that is a step off it.

- A stretch ends at a clock step longer than telemetry's detection floor,
  `step_us`. It was 200 us against the loop's 300 us callback; it is 100 us
  now, and `s1_detection_floor` refuses a floor above half the callback, read
  from both images' start lines, because a preemption below the floor would
  read as CPU time and a correct S1 as a leak.
- S1: no telemetry release may hold the CPU past 2 ms.
- Control: most releases must (the 5 ms burn runs whole), which shows the
  measure can see it. "Most", not "all": a host stall can split a release.
- `PREEMPT_RT`: also zero loop misses, as before.

**The slack is stated, and it is smaller than before.** The old bound let a
leak through as long as the loop's miss rate stayed 50 times below the
control's, i.e. about 2 % of telemetry releases running in front (and less on
a host whose own floor used up the budget). The new one tolerates no leaked
release: a single telemetry release that holds the CPU past the loop's miss
gap fails it. The loop's own miss count no longer judges on a generic kernel;
the test prints it for the record.

**Margins over every S1 console available** (rt-sched job of each
host-tests run since the B33 failure; only failing runs print the full
console, so `max_run_us` cannot be read retroactively and the column shows
what the old fields prove):

| Run (head) | Host | Control misses | S1 misses | S1 telemetry releases the loop preempted |
| --- | --- | --- | --- | --- |
| 45422896471 (`730dfbebeb`) | GitHub-hosted | 90 / 8,000 | 0 / 59,000 | console not printed |
| 45533251762 (`939ac840b2`, the B33 red) | GitHub-hosted | 92 / 8,000 | 0 / 45,000 | 600 of 600 (wall - CPU 1,252-2,570 us; the loop released every 1.3 ms then) |
| 45538275999 (`939ac840b2`) | GitHub-hosted | 96 / 9,000 | 0 / 59,000 | console not printed |
| 45551837174 (#4851, the fix) | GitHub-hosted | 96 / 8,000 | 0 / 60,000 | console not printed |
| 45596611184 (`cffdca5c9b`) | GitHub-hosted | 94 / 8,000 | 55 / 59,000 | 600 of 600 (wall - CPU 2,149-3,139 us) |

Under the new rule the margin is the gap between the longest telemetry stretch
in S1 and the 2 ms line, against the control's ~5 ms. Measured on host 118
(Linux 6.8 generic, the test's docker launcher, `taskset -c 0`, `step_us`
100), three solo runs of the test at `cffdca5c9b` plus this change:

| Run | S1 `max_run_us` min / median / max | S1 loop misses | Control `max_run_us` min / median / max | Control misses |
| --- | --- | --- | --- | --- |
| 1 | 694 / 694 / 695 | 0 / 60,000 | 5,000 / 5,000 / 5,000 | 98 / 9,000 |
| 2 | 694 / 695 / 696 | 0 / 60,000 | 5,000 / 5,000 / 5,000 | 100 / 9,000 |
| 3 | 694 / 695 / 696 | 0 / 60,000 | 5,000 / 5,000 / 5,000 | 101 / 9,000 |

S1's longest stretch is 696 us, the 1 ms period minus the loop's ~300 us
callback, as predicted: 1,304 us under the line. Every control release ran its
5 ms burn whole, 3,000 us over it.

## Resolved (2026-11-27)

The park fix (#4851, `8c936f649a`) and S1's verdict on telemetry's longest
CPU stretch (#4863, `5058187aec`) are both on main. Since then the
`real-time levels (rt-sched)` job has concluded success in every host-tests
run on main: 45684811335 (`01388e07a3`), 45742787244 and 45747295109
(`d92c369a14`), 45820896742 (`ebbb3a570d`) and 45868135916 (`52fcb58126`).
The millisecond conversions left on purpose went to issue 2967.

