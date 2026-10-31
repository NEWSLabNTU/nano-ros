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

- `contract_monitors::hazard_rung` picks the top rung whose output a rate row
  of the executor's table publishes, provided it carries a `route_ms`
  (rounded up). It does not fall through to a lower rung when the top observed
  one has none: the top rung answers first while it is available.
- `HazardSpec` gains `rung_row` and `rung_route_ms` (0 = none, which is what
  an older C/C++ entry's zero-filled trailing fields read as);
  `nros_cpp_hazard_row_t` gains the same two.
- `check_hazard` stamps the fire (the detection tick) and the rung output's
  count, and judges once per detection: nothing when the output publishes
  within the route, `hazard-route-runtime` (measured from the fire, declared
  `route_ms`) when it publishes late or the route passes with none. A
  recovery before the rung answers ends the watch.
- `ReactionEvent` (and `nros_cpp_reaction_event_t`) gains `fired_at_us`, the
  monotonic µs the verdict fired; for `hazard-detected`, the detection tick.

Not done: per-mode tracking of which rung is active. A fault that lands while
a lower rung is the active one is judged against the top rung's route.
`starts_at` is not read; the fire is the tick this executor detected the
hazard on.
