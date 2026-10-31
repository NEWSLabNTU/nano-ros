---
id: 2173
title: "The image measures one dispatch where play_launch's checker charges a rung a route, and nothing says the two halves state different quantities"
status: resolved
type: enhancement
area: [executor, codegen, orchestration]
found: 2026-10-31
related: [phase-462, phase-474, phase-503]
resolved_in: "phase96-hazard-route-runtime (play_launch phase 96 W4.2)"
---

## The gap

play_launch's fault-reaction arithmetic charges each rung of a hazard's
ladder a ROUTE: from the detection firing to the rung's first output
(play_launch phase 85 D4). The image had no such monitor. It had
`max-latency-runtime` (one dispatch, phase-474 I6), the supervised routes
(`route-latency-runtime`, `route-deadline-runtime`: take of a trigger to an
output), and `hazard-reaction` (the `safe_state` publisher's count moving
after the fault, judged against the FTTI). None of them measures the
quantity the checker states per rung, and the model carried no ladder for the
image to read: `HazardContract.reaction` was one path.

## Resolution

rlm v0.3.0 added `HazardContract.rungs` (`rung`, `role`, `route_ms`,
`starts_at`, `output`, top rung first; a rung the fault removes is not
listed), and play_launch 442faaa3 writes them.

- Every rung reaches the executor (`contract_monitors::hazard_rungs`,
  `RungSpec`, `nros_cpp_hazard_rung_t`): its mode id, the rate row observing
  its output, its `route_ms` rounded up (0: not judged), its role, and the
  guard members that belong to its `starts_at` node.
- `check_hazard` takes the mode the executor judges by. At detection the
  active rung is the one whose mode the executor is already in (the ladder
  fell before this fault), else the top rung; while the hazard holds,
  entering a lower rung's mode is a fall and makes that rung active. The
  active rung's route clock starts at its `starts_at` detection when that
  node is one of the guard's detectors here, else when it becomes active
  (a rung below a window: its `starts_at` is the window owner). It is judged
  once: nothing when its output publishes within the route,
  `hazard-route-runtime` (measured from the clock start, declared its
  `route_ms`) when late or never. A recovery ends the watch.
- `ReactionEvent` (and `nros_cpp_reaction_event_t`) gains `fired_at_us`, the
  monotonic µs the verdict fired; for `hazard-detected`, the detection tick.

Not done: a rung change the application makes without `request_mode` is
not seen; the active rung is the mode the executor applied.
