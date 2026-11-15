---
id: 2619
title: "A scheduling profile realizes the same way on every port; there is no per-port override"
status: open
type: limitation
severity: low
area: [orchestration, platform]
found: 2026-11-15
related: [2620]
---

**Owner:** unassigned. **Home:** RFC-0079 "Open questions" (per-port override).
Phase-459 is archived.

## What happens

RFC-0079's profiles (`realtime_periodic`, `responsive`, `throughput`,
`background`) are presets over the timing contract. Their realization is one
rule for every port: ORDER is derived from the contract, and the realizer takes
the k-th most urgent priority of the port's `pool.app`
(`PriorityPlan::nth_app_priority`, `nros-orchestration-ir/src/priority_plan.rs`).

Pools differ by two orders of magnitude: FreeRTOS's is `[1, 3]`, ThreadX's
`[15, 31]`, NuttX's `[1, 99]`. A `responsive` sched task may want a different
realization on a 3-level pool than on a 99-level one, for example sharing a
level with its neighbour rather than taking the next one. Today it cannot ask
for one.

## What closes it

A decision first: whether a profile may carry per-port realization, and if so
where it is stated (the port's plan, or the profile's definition). Until then
the uniform rule stands; on a pool narrower than the rank count the realizer
already clamps loudly and records a `Degradation`.
