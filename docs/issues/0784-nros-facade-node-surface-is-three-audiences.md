---
id: 784
title: "`nros::` publishes three different audiences under one namespace — the
  component API a user writes, the machinery `nros::node!` expands into, and
  four types nothing consumes"
status: open
type: tech-debt
area: api
related: [rfc-0043, rfc-0044, phase-379, issue-0783]
---

## Problem

`packages/api/nros/src/lib.rs:222` re-exports 25 items from
`packages/api/nros/src/node.rs` at the top level of `nros::`. That module's own
first line says what it is:

```rust
//! Rust component API shared by metadata discovery and generated runtimes.
```

Three audiences, one namespace, no marking:

**1. The component API a user writes.** `Node` (a TRAIT), `ExecutableNode`,
`NodeContext`, `CallbackCtx`, `Callback`, `NodeOptions`, `NodeResult`,
`TickCtx`. `examples/native/rust/talker/src/lib.rs:21` imports exactly these:

```rust
use nros::{
    Callback, CallbackCtx, ExecutableNode, Node, NodeContext, NodeOptions, NodeResult,
    TimerDuration,
};
```

**2. Machinery `nros::node!` expands into.** `DeclaredNode`, `register_node`,
`NodeId`, `record_node_metadata`. A user never writes these; the macro resolves
them through `::nros::`, which is why they are public. One of the module's items
is already handled correctly — `__private_node_state_into_raw` is `#[doc(hidden)]`
with a comment saying it exists for the macro — so the pattern for the rest is
established and unapplied.

**3. Types with no consumer at all.** Counting uses outside the defining module:

| item | consumers in `packages/` + `examples/` |
| --- | ---: |
| `NodeRuntimeAdapter` | 0 |
| `RuntimeNodeRecord` | 0 |
| `NodeSlot` | 0 |
| `MISSING_NODE_EXPORT_ERROR` | 0 |
| `DeclaredNodeRuntime` | 1 |
| `ActionExecutor` | 1 |
| `PublisherResolver` | 2 |
| `ClientDispatch` | 2 |

## Why it matters

`nros::Node` is a trait a component implements. It is NOT the handle entities
are created on — that is `nros_node::executor::node::NodeCtx`, which the facade
does not export at all. A Rust user reading `nros::Node` to find rclrs's `Node`
finds something with a `register` method and no `create_publisher`.

Phase 379's correlator reports 45 open decisions in the Rust `node` stage, and
most of them are this: items that have no rclrs counterpart because they belong
to a different model, mixed with items that have no counterpart because nothing
uses them. Those need opposite answers, and today they look identical.

This is the concrete half of the "the facade exports 709 items rclrs has no
equivalent for" finding recorded in phase-379's first report. That framing was
too simple: a large share of the 709 is the component model, which is
deliberate, user-facing and correct. The problem is that it is not
distinguishable from the machinery beside it.

## Not a bug in the component model

RFC-0043/0044's declarative shape is a deliberate divergence from rclrs's
handle model — a user declares a component and receives callbacks rather than
holding a node and calling methods on it, because entity storage is static and
the executor owns dispatch. Phase 379 records it as `divergence` with that
constraint. Nothing here argues against it.

## Direction

Three separable moves, none of them decided here:

* Mark the macro-expansion items `#[doc(hidden)]`, as
  `__private_node_state_into_raw` already is. Cheap, and it makes `nros::`'s
  rustdoc the component API rather than the component API plus its plumbing.
* Decide what the four zero-consumer types are for. If they are a public seam
  someone needs, they need a doc comment saying so; if they are leftovers,
  delete them.
* Decide whether `NodeCtx` should be exported. It is the type entities are
  created on and it is unreachable from `nros::`; see also issue 0783 for the
  same shape in the error vocabulary.

Phase 379 W5 owns the facade's export policy and should settle all three at
once rather than piecemeal.
