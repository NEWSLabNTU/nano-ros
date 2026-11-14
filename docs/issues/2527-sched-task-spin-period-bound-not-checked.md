---
id: 2527
title: "A sched task whose spin period is longer than its tightest timer is not warned about"
status: open
type: limitation
severity: low
area: [codegen, orchestration]
related: [rfc-0032, rfc-0015]
found: 2026-11-14
---

**Found by** the phase-555 W3 review of RFC-0032 before it moved to Stable.
RFC-0032 §8 listed this as open item 3 ("low priority"); it is the only one of
its five open items that the code has not answered, so it moves here and the
RFC points at this issue.

## What is missing

A sched task's `spin_period` is declared in the bringup's `system.toml`
(`examples/workspaces/realtime-c/src/demo_bringup/system.toml` has two). A timer
in that task cannot fire more often than the task spins, and RFC-0015 §4.3 lists
"Sched task `spin_period_us` must be ≤ tightest timer period in its group set"
among the invariants codegen enforces. If `spin_period_us` is longer than the tightest timer period of any node
in the task, the timer silently runs at the spin rate instead of its own.

That invariant is not enforced: neither `nros::main!`
(`packages/core/nros-macros/src/main_macro.rs`) nor the entry emitters in
`packages/cli/nros-cli-core/src/codegen/entry/` compare the two numbers.

## What a fix needs

- The timer periods per sched task. The probed component metadata carries each
  timer's declared rate (`system.contract.yaml` `trigger: { timer: { rate_hz } }`),
  and the planner already groups nodes by sched task.
- A warning at plan time naming the sched task, its `spin_period`, the timer
  and its period. A warning rather than an error: an image may accept the
  slower rate on purpose.
