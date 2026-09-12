---
id: 1203
title: "`Executor::parameter`'s `ParameterBuilder` declares straight into the
  store, so `use_sim_time` named through it attaches no clock source"
status: open
type: bug
area: core
severity: low
related: [1202, phase-425, phase-426, phase-430]
found: 2026-09-07
---

## What

`Executor::parameter::<T>(name)` (`packages/core/nros-node/src/executor/spin.rs`)
hands out a `nros_params::ParameterBuilder` holding `&mut p.server`. Its
terminal verbs (`.mandatory()`, `.optional()`, `.read_only()`) declare THROUGH
that borrow, so the executor never observes the write.

`use_sim_time` is a reserved parameter: the executor, not the app, acts on it
(`note_reserved_parameter` → `refresh_use_sim_time_from_store` →
`reconcile_ros_time_source`). Both `declare_parameter` paths call the hook —
issue 1202 fixed their ordering and added the missing one — but the builder
reaches neither, so:

```rust
executor.parameter::<bool>("use_sim_time")?.default(true).mandatory()?;
```

stores `use_sim_time = true`, lists it over `ros2 param list`, and attaches no
`/clock` subscription. The parameter reads as on and does nothing, which is the
same "sim time on, no clock source" state 1202 described, arrived at by a
different door.

## Why it was not fixed with 1202

The builder holds an exclusive borrow of the server for its whole lifetime, so
there is no point at which the executor can be told what it declared. Closing
this needs one of:

* the builder's terminal verbs returning through the executor rather than the
  server (a signature change on a public API), or
* the reconcile reading the store when it has no recorded opinion — cheap only
  once phase-430 W2 declares `use_sim_time = false` on every node, after which
  `sim_time_stated` is always true and the read is not a per-spin scan for
  images that never mention the parameter.

W2 is the natural home. Until then, declare `use_sim_time` through
`Executor::declare_parameter` / `declare_parameter_with_descriptor`, both of
which carry the hook.

## Reproduction

None in-tree: nothing in the repo declares `use_sim_time` through the builder.
The gap is reachable by any consumer that uses the rclrs-shaped verb.
