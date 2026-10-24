---
id: 2020
title: "On a Rust level image every component's tick runs on the boot (least urgent) level"
status: resolved
type: limitation
area: [executor]
severity: low
found: 2026-10-24
resolved: 2026-10-24
related: [phase-509, rfc-0108, 2016]
---

## Measured

`nros::levels::LevelCore` (phase-509 P10) shares one `ExecutorNodeRuntime`
across the levels. Spawned levels run band passes
(`Executor::spin_band_once`), which skipped the executor-wide chores,
component ticks included. Every tick ran in the boot level's pass: the
executor's enrolled components at the tail of `Executor::pass`, the runtime's
own in `ExecutorNodeRuntime::spin_once_shared`. So a component registered by
an urgent level ticked at the least urgent level's priority and period. That
covered its service-client polls and its action-server completion and
feedback. Callbacks were not affected.

## Resolution

A component's tick now runs on the level that registered it.

- `nros-node`: `ComponentSlot` records the registering level
  (`Executor::set_registering_level` at `enroll_component`), as each slot's
  tag does. `Executor::pass_ticks_owner(band_level, owner)` is the one rule:
  on an executor whose levels own their slots, a band pass ticks only its
  level's components, and the executor's own pass (the boot level) ticks its
  own level's and any registered before a level was set. Without level
  ownership nothing changes: the executor's own worker ticks everything and
  a band pass nothing. `spin_band_once` runs its level's ticks at both of its
  exits (`Executor::tick_components`). `BandWorker::level` and
  `Executor::registering_level` are new getters.
- `nros`: `ExecutorNodeRuntime` keeps a level per `register_node` component
  (`component_levels`) and `run_ticks_for` applies the same rule.
  `LevelCore::spin_level_once` ticks its level's runtime components after the
  band pass, under the lock.
- Locking (RFC-0108): the lock stays held across every tick, as before, and
  each component belongs to one level, so no component is ticked by two
  threads. The C-ABI level runners run the same band pass, so a Rust node in
  a C or C++ level image gets the same behaviour.
- RFC-0108 "As built in P10" states the rule.

Tests (`nros-node`, `executor::tests::p9b_windows`):
`a_component_ticks_on_the_level_that_registered_it` (per-level ticks, plus
the unchanged no-level control) and
`an_urgent_levels_action_server_completes_on_its_own_level`: an action server
and its completing component registered by level 0. The goal is accepted by
level 0's band pass and completed by the tick that pass runs, on level 0's
thread. The boot level spins beside it on another thread and neither accepts
nor completes the goal. Both tests fail with the old rule restored.

The fixture is a host executor test, not a board image. The only Rust
`LevelCore` runners are Zephyr and ThreadX; the Rust Linux arm is still one
executor per tier (issue 2016).
