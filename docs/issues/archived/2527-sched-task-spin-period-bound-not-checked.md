---
id: 2527
title: "A sched task whose spin period is longer than its tightest timer is not warned about"
status: resolved
type: limitation
severity: low
area: [codegen, orchestration]
related: [rfc-0032, rfc-0015, rfc-0079]
found: 2026-11-14
resolved: 2026-11-19
resolved_in: "fix(issue 2527): nros::main! derives and holds a sched task's spin period as the C/C++ entries do"
---

**Found by** the phase-555 W3 review of RFC-0032 before it moved to Stable
(RFC-0032 §8 open item 3).

## What was missing

A timer in a sched task cannot fire more often than the task spins.
RFC-0015 §4.3 lists "sched task `spin_period_us` must be at most the tightest
timer period in its group set" among the invariants codegen enforces, and
nothing compared the two numbers.

## Where it stood when picked up (2026-11-19)

The C/C++ entry road already enforced it, as a refusal rather than the
warning this issue proposed. RFC-0079's allocation table (`spin_period`, in
`nros_orchestration_ir::spin_period`) derives an unset spin period from the
task's fastest member release (each member's tightest timer trigger, the
realizer's `node_periods_us`), and refuses an authored one slower than that.
`nros` codegen's entry road called it, and `nros::main!` did not:
- an unset spin period was baked as 1000 us;
- a spin period longer than a member timer was kept, and that timer ran at
  the spin rate in silence.

## Fix

- **One shared entry.** `spin_period::apply_model_spin_periods(model, table)`
  reads the periods from the model and applies the rule.
- **Both roads call it.** The CLI entry road moved onto it, and
  `nros::main!` now calls it after resolving its sched-task table.
- **The refusal names** the sched task, both periods, the member's timer,
  and the `[tasks.<name>] spin_period` setting to change.

It is a refusal, not a warning, to match the C/C++ road and RFC-0079: an
image that wants a slower spin can lower the timer's rate instead.

No in-tree Rust image is refused. The three Rust bringups that author a spin
period (`realtime-rust` demo and the two `orchestration_sched_tasks_*`
fixtures) declare no timer releases. `realtime-rust`'s derived bringup now
spins at its members' periods instead of 1 ms. `changelog.d/2527.breaking.md`
records the change for Rust users.

## Tests

`spin_period::tests`:
- `the_model_entry_derives_from_the_member_timer`;
- `the_model_entry_refuses_a_spin_period_longer_than_the_member_timer`
  (a 30 Hz timer model; the refusal text is checked for all four facts).

The C/C++ road's `derived_sched_tasks_entry` spin tests still cover that
road.
