---
id: 371
title: native_sim cyclone app abort()s ~19-21 s into an Autoware-graph session during MRM/service churn
status: open
type: bug
severity: high
area: rmw
related: [issue-0377, issue-0267]
---

# 0371 — native_sim cyclone app abort()s ~19–21 s into an Autoware-graph session

**Status:** Open
**Filed:** 2026-08-01
**Affects:** `nros-rmw-cyclonedds` on Zephyr native_sim joining a large
(~40-participant / 68-composable) stock ROS 2 Humble graph
(simple-autoware-safety-island direct-connection demo)

## Symptom

The safety-island image (4 ported nodes, cyclone RMW, domain 1 direct)
dies with a bare `abort()` → `ZEPHYR FATAL ERROR 4` at a near-deterministic
sim-time (18.7–21.0 s across 7 reproductions on 2026-08-01), always during
the demo scenario's INit phase. Immediately before death the mrm_handler is
FLAPPING `NORMAL -> MRM_OPERATING -> NORMAL` (operate/cancel requests to
the operator services each cycle), so the cyclone service-call path is hot.
The aborting thread is an unnamed cyclone pthread (CONFIG_THREAD_NAME shows
`(unknown)`; two distinct stack addresses seen). No cyclone warning is
traced before the abort (baked `<Tracing><Verbosity>warning</Verbosity>`).

## What does NOT reproduce it (all ≥60–90 s clean)

- island alone;
- island + one rclpy peer feeding availability + odometry @10 Hz (451 msgs);
- island + the same peer with availability FLAPPING 0.5 Hz (service churn
  driver, 701 msgs);
- island + the FULL sim graph idle (no scenario);
- island + sim + `/initialpose` + EKF odometry @40 Hz (`ros2 topic hz`
  confirmed) — survives;
- under `gdb -batch -ex run` the full failing scenario runs to completion
  (timing-sensitive: the debugger masks it).

## What DOES reproduce it (7/7 on 2026-08-01)

`just demo-all` (or sim + island + `demo/scenario_driver.py` by hand) in
simple-autoware-safety-island @ direct-connection HEAD, nano-ros at
`929dee182` OR at `471a62529` — and the SAME tree passed this scenario
twice on 2026-07-31 (sim then reported 32/33 nodes vs today's 33/33;
`autoware_manual_lane_change_handler` had crashed at startup that evening,
apport log 20:58). Suspected trigger: some participant/endpoint present
only in the 33/33 graph interacting with the scenario driver's endpoint
churn (its `latest()` helper creates and destroys a subscription per poll).

Reproduces under `strace -f -k` (stacks unwind only to zephyr's
`posix_print_trace` shim — the abort caller is above the custom zephyr
thread stack, invisible to the strace unwinder). No core: apport ignores
non-package binaries.

## Trigger CONFIRMED (2026-08-01): `autoware_manual_lane_change_handler`

A/B on the live stack: overlay-shadowing the node out of
`tier4_planning_launch` (demo commit; same mechanism as the MRM shadow)
turns 7/7 deterministic aborts into a clean `VERDICT: PASS` on the first
try — with the island built from the pinned submodule. This also explains
the 07-31/08-01 flip: on 07-31 the node happened to crash at startup
(apport 20:58, sim 32/33), so the passing runs never saw its endpoints.

The node's surface (launch remaps): subscribes the lanelet `vector_map`
(multi-MB transient_local) and `/localization/kinematic_state`; exposes the
manual-lane-change services/state used by its RViz plugin. Which of its
endpoints kills the island's cyclone session is the open question — the
island subscribes neither of its inputs, so suspicion falls on its
service/TL endpoint announcements interacting with the island's SEDP
handling at scale.

## Next steps

- Get the abort site: wrap zephyr's `abort()` (link-order stub printing a
  `backtrace()` from the aborting pthread before panicking), or run under
  `rr` if available.
- Suspect list, in order: cyclone service req/rep path under churn
  (handler operate/cancel every state flap), SEDP proxy create/dispose
  churn from the scenario's subscription-per-poll pattern, ddsrt alloc
  failure that 256 MiB arena + 16384 mutex/cond pools did NOT absorb.
