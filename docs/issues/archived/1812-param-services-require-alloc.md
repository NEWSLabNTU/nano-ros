---
id: 1812
title: "`param-services` still requires `alloc`, so an image with no heap cannot answer `ros2 param`"
status: resolved
type: limitation
area: [params, node]
severity: medium
found: 2026-10-11
resolved: 2026-10-12
related: [phase-382, phase-426, 0756, 1706, rfc-0105, phase-491]
---

## Resolution (2026-10-12)

The parameter services are carved from the executor backing, beside the
store's slots, and `param-services` builds without `alloc`. The
`compile_error!` is gone. This also lands RFC-0105 D10.2.

- **One region, `ParamHome`.** A new backing region placed after the store's
  slots (`nros-executor-layout` `RegionUnits::param_home` / `Counts::param_home`;
  `nros-node` `executor::storage::param_homes`). It is carved exactly when the
  sizing carves a store (`params > 0`), so an image with no store pays nothing
  (issue 1702's rule) and the stated-reservation arithmetic
  (`PARAMETER_STORE_U64S`) picks it up with no new knob. It holds:
  - item 1: the `ParamState` itself, built in place (`ParamState::init_at`);
  - item 3: the service sets, now INLINE in the state
    (`heapless::Vec<ParameterServiceServers, MAX_SERVICE_SETS>`). The
    `Box<dyn ParamServiceProcessor>` and the trait are gone; there was one
    implementation;
  - item 4: the shared request/reply pair, two `[u8; HOME_BUFFER_BYTES]`;
  - D10.2: the staging slot (`ParamState::staged`). A remote set decodes the
    incoming value into it (`read_parameter_value_into`) and applies it with
    `ParameterServer::apply_staged`, which runs `apply`'s rules and hooks and
    then SWAPS the slot with the stored value.
- **Item 2.** With `alloc` off, the default sizing must carve a store: a build
  where `nros_params::IMPLIED_STORE_SLOTS == 0` fails at compile time with a
  message naming `NROS_PARAM_STORE=1` / `NROS_MAX_PARAMETERS` / `alloc`. An
  executor opened with an explicit sizing that carves no store gets the
  existing issue-1706 refusal at the first declaration. With `alloc` on, the
  heap fallback stays: the store is leaked as before, and the state is one
  fallible allocation initialised in place (`ParamState::try_boxed`).
- **Also gated on `alloc` now:** argv parameter overrides
  (`install_argv_params`, `apply_ros_args_with_params`). They are a hosted-edge
  input and keep their heap.
- **Lane.** `scripts/check-param-services-heap-free.sh`, run by
  `just check compile-smoke`: `nros-node --no-default-features --features
  rmw-cffi,param-services` must build with `NROS_PARAM_STORE=1`, and must be
  refused by name without it. Before this change the positive row failed on 38
  `alloc` errors.

### Measured (x86_64)

Region and type sizes in the heap-free `rmw-cffi` build, default knobs
(`-Zprint-type-sizes`):

| | bytes |
| --- | --- |
| `ParamHome` (whole region) | 32,000 |
| `ParamState` (in it) | 23,808 |
| `ParameterServiceServers` (one set, x `MAX_SERVICE_SETS` = 4) | 3,696 |
| `ParameterValue` (the staging slot) | 8,464 |
| request + reply buffers | 2 x 4,096 |
| `Executor` | 2,264 (bound 6 KiB) |

Stack used by one streamed set (one integer + one 32-element string array),
measured by stack painting in
`a_remote_set_holds_no_parameter_value_on_the_stack`:

| profile | handler | before | after |
| --- | --- | --- | --- |
| dev (opt-level 1) | `set_parameters` | 43,160 B | 872 B |
| dev (opt-level 1) | `set_parameters_atomically` | 51,688 B | 984 B |
| release | `set_parameters` | 26,472 B | 352 B |
| release | `set_parameters_atomically` | 35,016 B | 464 B |

The test asserts the handler stays under one `ParameterValue`, and a control
test proves the painter sees a frame that does hold one.

### Left as it was

- The sets are inline for every node slot (`MAX_SERVICE_SETS`), not boxed per
  served node, so a carved home costs the full table. That is 4 x 3,696 B at
  the default, against the 285 KB store it rides with.
- The undeclared add (`allow_undeclared`) still writes one empty entry by value
  in `ParameterServer::declare_placeholder`, out of line so it is not on the
  frame of an ordinary set.
- `lifecycle-services` keeps its `compile_error!`; its processor is still boxed.

## What is true

The parameter STORE does not need a heap. `nros-params` builds with
`--no-default-features`, and since phase-382 W3' an executor whose backing has
store slots carves the table from them. The heap is not asked.

The SERVICES still need a heap, so `nros-node` refuses the combination:

```rust
// packages/core/nros-node/src/lib.rs
#[cfg(all(feature = "param-services", not(feature = "alloc")))]
compile_error!("`param-services` allocates: add \"alloc\" to this crate's features");
```

Phase-382 named this as its goal ("an image with no heap will be able to
answer `ros2 param`") and archived without reaching it. This issue carries the
remaining work, phase-382 W4'.

## The heap reasons left, measured on main 2026-10-11

Each one is about PLACEMENT, not dynamic size. That is why the work is
tractable.

| # | where | what | size |
| --- | --- | --- | --- |
| 1 | `nros-node/src/executor/spin.rs` `Executor.params` | `Option<Box<ParamState>>` | tens of bytes (W2' made the server a table borrow plus a count) |
| 2 | `spin.rs` `leak_parameter_storage` | store fallback when the backing carves no slots (W3' made the region CONDITIONAL) | 285,184 B at `MAX_PARAMETERS=32` |
| 3 | `nros-node/src/parameter_services.rs` `ParamState.services` | `heapless::Vec<Box<dyn ParamServiceProcessor>, MAX_SERVICE_SETS>` | one box per node's service set |
| 4 | `parameter_services.rs` `ParamServiceBuffers` | `request`/`reply: Box<[u8]>`, shared per executor | 8,192 B per executor |

These are NOT reasons any more, so do not chase them:

- The boxed reply handlers (`handle_get_parameters` & co., ~line 940) are
  `#[cfg(test)]`. They are the oracle the streaming handlers are compared
  against, and are on no live path.
- `SetParametersRequest` (1,192,968 B) is no longer a stack local. W1''s
  streaming seam reads requests from the wire buffer.

## What closing it means

- Items 1, 3 and 4 move into the executor backing, the same carve mechanism
  W3' used for the table (`ExecutorSizing` / `carve`).
- Item 2: with `alloc` off, a backing with no store slots is a build error
  that names the knob. It must not silently fall back to the heap.
- Delete the `compile_error!`. Add a lane that builds `nros-node` with
  `param-services` and without `alloc`. Per CLAUDE.md, a target nothing
  builds is not coverage (`check-required-features-reachable`).
- `lifecycle-services` carries the same `compile_error!` for a smaller reason
  (`lifecycle_services.rs` boxes its processor). It is out of scope here and
  is recorded so the two are not confused.

## Constraint carried over from phase-382

`executor_stays_small_enough_to_construct_on_a_stack` bounds
`size_of::<Executor>()` at 6 KiB. Anything moved off the heap must be CARVED
from the backing, never held inline in `Executor`. Inlining the 8 KiB buffers
trips that test at once, and inlining the store trips it by 285 KB.
