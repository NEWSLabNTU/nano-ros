---
id: 746
title: "Executor timer over-credits under subscription load — a 30 ms timer publishes at ~50 Hz with real traffic, exact at 31.6 ms standalone"
status: open
type: bug
area: core
related: [issue-0745]
---

# 0746 — timer over-credit under load

Split out of 0745's "left open" list, with the measurements that isolate
it. On the ASI reference consumer (freertos-posix board, cyclonedds RMW,
RFC-0047 tier-bound timer, 30 ms period seeded via 0745):

- **Standalone** (no subscription traffic): timer callback intervals
  mean **31.6 ms**, min 31.4, max 32.5 (n = 1265) — the period plus
  ~1.5 ms service jitter. Correct.
- **Under load** (five subscriptions live against a real Autoware
  planner: 8.8 KiB trajectories at 10 Hz + kinematic state + accel +
  steering + op-mode): the SAME timer's topic measures **~50 Hz**
  (`ros2 topic hz`, window 1230): min interval ~0 (bursts of several
  fires), max interval ≈ the 30 ms period, average 1.5× the configured
  rate. Sustained over a 25 s window — this is not bounded catch-up
  (drift-free catch-up cannot exceed 1/period on average), it is genuine
  over-crediting that scales with event traffic.

Earlier ASI measurements fit the same curve at the old effective period
(150 ms → 12.4 Hz observed ≈ 1.9×; tiers changed the shape but not the
class).

## Where to look

The spin loop's timer crediting when callbacks/events consume loop time:
the hosted pacing path (`session_drive_io` sleeps `timeout_ms`; the
runtime credits elapsed wall-clock) plus per-iteration credit — the
suspicion is a DOUBLE credit source when the loop both (a) measures real
elapsed time and (b) assumes the drive_io timeout elapsed, or a
fire-when-behind rule that resets the deadline to `now` instead of
`deadline += period` (which turns every late tick into a rate increase
rather than a one-off catch-up... note that would UNDER-shoot, not
overshoot — the 1.5× direction points at double-credit).

A minimal upstream repro should not need ASI: the realtime-cpp workspace
(ctrl 10 ms tier) with a flood publisher into a listener group on the
same executor, `ros2 topic hz /ctrl` vs standalone.

## Why it matters

For a control node the publish rate IS the control rate — a controller
configured at 33 Hz emitting 50 Hz under load changes actuator dynamics
and invalidates WCET/tier budgets (phase-357). ASI's soak (phase-4) is
gated on this.
