---
id: 746
title: "Executor timer over-credits under subscription load — a 30 ms timer publishes at ~50 Hz with real traffic, exact at 31.6 ms standalone"
status: wontfix
type: bug
area: core
related: [issue-0745, issue-0744]
---

# 0746 — timer over-credit under load (CLOSED: measurement artifact, executor exonerated)

**Resolution (2026-08-21): not a bug.** The ~50 Hz reading was `ros2 topic hz`
aggregating MULTIPLE island processes publishing the same topic — stale
`actuation_posix_entry` instances left over from earlier debug runs on the same
host and domain. With exactly one island process, the timer is exact under the
same load. The executor's timer accounting was instrumented and verified
credit-exact; nothing in nros needs to change.

## The verification that closed it

Temporary instrumentation in `spin.rs` (per-spin credited `delta_us` sum vs the
platform tick clock vs host `CLOCK_MONOTONIC`, every 200 spins) and
`arena.rs::timer_try_process` (per-fire log), on the ASI freertos-posix
reference consumer, cyclonedds RMW, tier-bound 30 ms timer:

- **Credit is exact, standalone AND loaded**: every 200-spin window credited
  `dcred == dtick` to the microsecond (1 200 000 µs per 200 × 5 ms spins), with
  the full Autoware planning-simulator graph attached (8.8 KiB trajectories at
  10 Hz + kinematic state + accel + steering + op-mode, controller in the
  emergency-stop state — the busiest path). No double credit exists: the
  suspicion in the original filing (wall-elapsed + assumed drive_io timeout)
  is falsified; the hosted path credits measured wall clock only, and the
  telescoping `last_spin_end_us` sum cannot exceed elapsed clock time.
- **Single process under load**: timer fired 634 times in 20 s (31.7 Hz) while
  a simultaneous `ros2 topic hz` on the topic measured **31.669 Hz, min 31 ms,
  max 32 ms, std dev 0.06 ms** — fire rate and wire rate agree exactly.
- **The artifact reproduced**: three stale `actuation_posix_entry` processes
  (from an older build dir) were found alive and publishing on the same domain.
  With them running, the topic measured ~19 Hz with min-interval ~0 bursts and
  max ≈ period — the multi-publisher signature: independent phases produce
  near-coincident arrivals (min ~0) and the mean becomes the SUM of the
  publishers' rates, which is how a 30 ms timer "measures" 1.5×.
- **The historical datum confirms the class**: the earlier 150 ms-era reading
  of 12.4 Hz observed vs 6.3 Hz effective is 2 × 6.3 — two islands, same
  artifact, not "the same curve at the old period".

## What IS real (and simulator-only)

31.6 ms standalone for a 30 ms timer is the freertos POSIX port's tick-thread
pacing (`pthread_kill` + `usleep(tick)` — usleep overshoot makes the simulated
tick ~5.2 % slow vs real time; measured `dreal/dtick = 1.0524`). The executor
credits the tick clock, which is the correct clock for a FreeRTOS image; the
skew belongs to the simulator port, is constant, and does not exist on hardware
or QEMU-icount targets. Not actionable in nros.

## Lesson (the durable part)

Rate measurements against a shared DDS domain are only valid after proving the
publisher count: `ros2 topic info -v <topic>` (publisher count) or
`pgrep -a <entry>` BEFORE trusting `ros2 topic hz`. A hz reading that shows
min-interval ~0 bursts with max ≈ the expected period is the multi-publisher
signature — suspect a duplicate process before suspecting the timer.
