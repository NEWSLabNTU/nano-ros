---
id: 2969
title: "A Rust node's typed `declare_parameter::<T>` is refused (`AlreadyDeclared`) for a name its launch file seeds, once the image has a parameter store"
status: resolved
type: bug
severity: medium
area: [nros-node, params, api]
related: [phase-566, 1272, 2638, 1203]
found: 2026-11-25
resolved: 2026-11-25
resolved_in: "fix(params, issue 2969): a declaration adopts the launch seed of its name"
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

## Resolved, 2026-11-25

One rule in the store, every language's declaration calls it.

- **The mark.** A store entry carries `launch_seed`. Every seeding path
  declares through `Executor::seed_launch_parameter_on_ref`, which declares the
  value and marks it: the Rust board entry (`ExecutorSink::create_node`),
  nros-cpp's `seed_launch_text` (C and C++ entries both seed through it). A
  value the code declared is never marked.
- **The adoption.** `ParameterServer::adopt_launch_seed` (exposed as
  `Executor::adopt_launch_seed_on`): for a marked name it checks the declared
  type (`LaunchValueTypeMismatch`, a new `DeclarationError`, otherwise), then
  the declared range if a descriptor is given (`InitialValueOutOfRange`),
  attaches the descriptor, keeps the launch value and CLEARS the mark. An
  unmarked name answers `None`, and the caller declares as it always did.
- **Callers.** The Rust typed builder (`typed::declare`) adopts first, so
  `declare_parameter::<T>` reads the launch value over its default; a second
  typed declaration is still `AlreadyDeclared`, as rclrs. The Rust declarative
  route, C's `rclc` server and C++'s `declare_on_node` adopt through the same
  call. For C and C++ the outcome is what it was (same type kept, another type
  refused with the header's or nros-c's line), and their existing tolerance
  of the code re-declaring a name is unchanged. The declarative Rust route
  used to keep a seed of ANY type silently; a seed of another type is now
  refused, naming the parameter and the cause.

Tests: `nros-params` `a_typed_declaration_adopts_its_launch_seed_once`
(adopt once, then duplicate; type mismatch; out of range; an unmarked name is a
duplicate); `nros-node` `a_launch_seed_is_adopted_by_the_first_declaration`
(the typed builder on a node, and the `adopt_launch_seed_on` seam the
declarative route, C and C++ call); nros-cpp `param_store_full_runtime`
(a C++ `declare_parameter<int64_t>` over a seeded `"20"` reads 20 and latches
nothing). The C road's adoption is end-to-end in `probe_seeds_workspace.rs`
(`seeds_c_pkg` declares `ctrl_period_ms` with `rclc_add_parameter` over its
launch seed). Each Rust test fails with the typed builder's adopt step removed.
