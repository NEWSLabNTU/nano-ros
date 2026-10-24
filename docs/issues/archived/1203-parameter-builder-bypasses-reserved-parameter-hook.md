---
id: 1203
title: "`Executor::parameter`'s `ParameterBuilder` declares straight into the
  store, so `use_sim_time` named through it attaches no clock source"
status: resolved
type: bug
area: core
severity: low
related: [1202, phase-425, phase-426, phase-430, phase-461]
found: 2026-09-07
resolved: 2026-10-18
---

**Home: [phase-461](../../roadmap/archived/phase-461-service-inbox-per-family.md)** — homed 2026-10-17; listed in that phase's "Issues homed here (2026-10-17)".

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

## Resolution (2026-10-18, phase-461 W7)

**The symptom had changed before the fix.** phase-430 W2 (`004a14309`) seeds a
`use_sim_time = false` placeholder on every node, and only the two
`declare_parameter` paths stepped it aside. Measured on `main` at
`6486216ab7`, the reproduction above returned `Err(TypeMismatch)` from
`.mandatory()` and the store kept the seeded `false`: the app's declaration was
refused, not silently accepted.

**Fix.** `Executor::parameter_on` steps the placeholder aside for the reserved
name before it hands out the builder, and marks the node
(`ParamState::sim_time_via_builder`). The next spin reads the store back
(`settle_builder_reserved_parameters`): if the builder declared the name, it runs
the same reserved-parameter hook `declare_parameter` runs; if the builder was
dropped without declaring, the placeholder goes back. No public signature
changed. This is the issue's second option, whose precondition phase-430 W2 met.

**Tests** (`executor::tests`, `sim-time` + `param-store`):

- `use_sim_time_through_the_builder_attaches_the_clock_source`: the builder's
  `default(true).mandatory()` succeeds, the store holds `true`, and `/clock` is
  attached and armed after one spin. Negative control: with the `parameter_on`
  hunk switched off it fails with `Some(TypeMismatch)`.
- `an_undeclared_use_sim_time_builder_puts_the_placeholder_back`: a builder that
  declares nothing leaves the seeded `false` and attaches nothing.

The eight existing `use_sim_time` tests are unchanged and pass.
