---
id: 1812
title: "`param-services` still requires `alloc`, so an image with no heap cannot answer `ros2 param`"
status: open
type: limitation
area: [params, node]
severity: medium
found: 2026-10-11
related: [phase-382, phase-426, 0756, 1706, rfc-0105, phase-491]
---

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
