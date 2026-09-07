---
id: 1202
title: "`note_reserved_parameter` ran before the parameter store's verdict, so a
  REFUSED `use_sim_time` re-declaration detached the `/clock` source while the
  store kept the old value"
status: resolved
type: bug
area: core
severity: medium
related: [1203, phase-425, phase-430]
found: 2026-09-07
---

## What

`Executor::declare_parameter` (`packages/core/nros-node/src/executor/spin.rs`)
called `note_reserved_parameter(name, &value)` **before** `params.server.declare(
name, value)` and **unconditionally** — the hook recorded the caller's PROPOSED
value, then the store decided separately whether to take it.

`ParameterServer::declare` refuses a name it already holds (it returns `false`
and changes nothing; a set goes through `apply`/`set`, not through a second
declaration). So on a node that had declared `use_sim_time = true`:

```rust
executor.declare_parameter("use_sim_time", ParameterValue::Bool(true));   // accepted
executor.declare_parameter("use_sim_time", ParameterValue::Bool(false));  // REFUSED
```

the second call returned `false`, left the store reading `use_sim_time = true`,
and still set `sim_time_requested = false`. The next `reconcile_ros_time_source`
then called `time_source::set_active(false)`, so `/clock` samples stopped being
installed.

The result is a state the parameter cannot name: **sim time recorded as ON, no
clock source armed**, and nothing to reconcile it back — the reconcile believes
the switch, and the switch believes a write that never happened. A subsequent
`ros2 param get <node> use_sim_time` answers `true` while every ROS-time timer
runs on system time.

Found by the phase-430 survey (finding E) while making
`just check node-std-tests` compile on `phase-428-w10-qos-ssot`; PR #629 pinned
the behaviour in `use_sim_time_attaches_and_detaches_the_clock_source` with a
comment naming this item rather than fixing it there.

## The sibling, same class

`Executor::declare_parameter_with_descriptor` carried **no reserved hook at
all**. `declare_parameter_with_descriptor("use_sim_time", Bool(true), desc)`
stored the parameter and attached nothing, so the same declaration meant two
different things depending on whether the app passed a descriptor. Found by
sweeping the seam, not by a report.

## Scope

`use_sim_time` is the tree's ONLY reserved parameter —
`nros_node::time_source::USE_SIM_TIME_PARAM` is the only such constant, and
`grep -rn 'note_reserved\|RESERVED_PARAM\|use_sim_time' packages/core/nros-node/src
packages/core/nros-params/src` finds no sibling name. (ROS 2's other
statically-typed names — `start_type_description_service`, `qos_overrides.*` —
have no nano-ros implementation to switch.)

## Fix (phase-430 W3)

* The hook runs **after** the store answers and **only on acceptance**, in both
  declare paths.
* It takes only the NAME and reads the value back out of the store, through
  `refresh_use_sim_time_from_store` — the same read the runtime
  `ros2 param set` path uses. The switch therefore follows what the store
  HOLDS, never what a caller proposed, and the declare path and the wire path
  cannot disagree about the value.
* `refresh_use_sim_time_from_store` becomes `pub(crate)` so both callers, and a
  test that changes the store directly, share one read instead of a second
  spelling.

Tests (`packages/core/nros-node/src/executor/tests.rs`):

* `use_sim_time_follows_the_store_verdict_not_the_caller` — accepted `true`
  attaches; a REFUSED re-declaration leaves both the stored value AND the source
  alone; an accepted `false` through the store detaches; an accepted `true`
  re-arms. Both halves are asserted, because the defect was the two halves
  disagreeing: checking only one reads as correct on the broken code.
* `use_sim_time_declared_with_a_descriptor_attaches_the_clock_source` — the
  sibling path.
* `use_sim_time_attaches_and_detaches_the_clock_source` now turns the switch off
  through the store rather than through a refused re-declaration.

## Not fixed here

The `ParameterBuilder` path bypasses the executor entirely — issue 1203.
