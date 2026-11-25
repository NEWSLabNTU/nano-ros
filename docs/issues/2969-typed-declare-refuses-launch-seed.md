---
id: 2969
title: "A Rust node's typed `declare_parameter::<T>` is refused (`AlreadyDeclared`) for a name its launch file seeds, once the image has a parameter store"
status: open
type: bug
severity: medium
area: [nros-node, params, api]
related: [phase-566, 1272, 2638, 1203]
found: 2026-11-25
---

## What happens

A board's Rust entry seeds a component's launch `<param>` values on the first
node the component builds (`ExecutorSink::create_node`, issue 1272), through
`seed_launch_params_on`, which DECLARES each name in the store. It does so only
when the store already exists before `register`, i.e. the image declared
`[param_services]`.

The component then declares the same name the rclrs way:

```rust
let period = node
    .declare_parameter::<i64>("ctrl_period_ms")?
    .optional()?;   // Err(DeclarationError::AlreadyDeclared)
```

The typed builder's terminal calls `ParameterServer::declare_parameter`, which
answers `TypeMismatch` for a name the table holds, mapped to
`AlreadyDeclared`. So the launch value is never ADOPTED: the declaration fails.
rclrs (and rclcpp, which the C++ road follows) applies a launch override AT
declaration instead.

Only `use_sim_time` is exempt (issue 1203: `yield_seeded_use_sim_time` removes
the seeded placeholder so the builder can declare it). The declarative route
(`declare_parameter_for_name` & co.) tolerates an already-declared name, so it
does not hit this.

Where it does not happen: an image with no `[param_services]` has no store
before `register`, so nothing is seeded into it and the typed declaration
succeeds; but then it reads NO launch value, only its default. Either way a
typed declaration never gets its launch value.

## Measured

Reproduced on the transport-free `metadata` executor while writing phase-566
W4: seeding the store with `ctrl_period_ms = 20` and then
`declare_parameter::<i64>("ctrl_period_ms").optional()` panicked at
`AlreadyDeclared` (first draft of
`packages/rmw/metadata/tests/probe_launch_seeds.rs`). The board path calls the
same `seed_launch_params_on`.

Because of this, the Rust metadata probe (phase-566 W4) seeds the store only
under the board's rule (a store that already exists), never one it makes,
and carries the seeds through `ctx.param()` alone. Seeding a store the probe
made would have turned this bug into a probe failure for every Rust component
that types a launch parameter.

## Wanted

A typed declaration of a name the launch seeded adopts the seeded value as its
override (it wins over the builder's default, and its type is checked against
`T`), as rclrs does and as `use_sim_time` already does. A second declaration
by the CODE of the same name is still refused. That needs the store to tell a
launch seed from a code declaration (per node, like `sim_time_seeded`).

## Acceptance

- On an executor with a store, after `seed_launch_params_on(node, [("p",
  "20")])`, `declare_parameter::<i64>("p").optional()` returns `Some(20)`, and
  `.default(5).mandatory()` reads 20.
- A seed whose text does not type as `T` is refused naming the parameter.
- Declaring `p` twice from code is still `AlreadyDeclared`.
- The Rust metadata probe may then seed a store it makes, and
  `probe_launch_seeds.rs` asserts the typed read sees the seed.
