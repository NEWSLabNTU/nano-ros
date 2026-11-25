---
id: 2965
title: "A spawned level parked in whole milliseconds, so a 1 kHz level released every 1.3 ms"
status: open
type: bug
severity: high
area: [executor, levels, real-time]
related: [phase-162, phase-509, issue-1242]
found: 2026-11-25
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
