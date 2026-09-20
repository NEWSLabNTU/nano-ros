---
id: 1405
title: "`node_raw_id != 0` decodes the PRIMARY slot as \"no node\" at nine
  `nros-c` registration sites — the same conflation issue 1384 fixed one layer
  up, and the one `nros-cpp` closed with a node-id bias in issue 0312"
status: open
type: bug
area: [api]
severity: medium
found: 2026-09-21
related: [issue-1384, issue-0312, phase-156, phase-189]
---

## What is true

`packages/api/nros-c/src/executor.rs` turns a node's slot into an
`Option<NodeId>` like this, at nine registration sites:

```rust
let node_raw_id = if !subscription_ref.node.is_bound() { 0 } else { subscription_ref.node.node_id };
let node_id = (node_raw_id != 0).then(|| nros_node::executor::NodeId::from_raw(node_raw_id));
```

`NodeId::PRIMARY` is 0 and the FIRST node an executor builds takes it — the
fact issue 1384 turns on, asserted in words by
`packages/api/nros-c/tests/run/executor_param_node_keying.c`. So `None` here
means both "this node is legacy" and "this node is the primary slot of its
executor", and the registration path cannot tell them apart. Sites, all in
`executor.rs`:

* `(node_raw_id != 0).then(...)` — 4 (the subscription variants, the timer/arena
  path at ~2053)
* `if node_raw_id != 0 { … } else { … }` — 5 (service, client, action server,
  action client)

Read them with:

```
grep -n "node_raw_id" packages/api/nros-c/src/executor.rs
```

## Why this is NOT issue 1384, and why it was left open there

1384 was the `nros_node_t` PREDICATE — "does this node reach an executor" —
and its call sites pick a DISPATCH ARM. These nine pick an IDENTITY to register
under, which is a different decision with a different fix.

The fix shape is also different, and is why folding it into 1384 was rejected:
`nros_node_t::node_id` is public `repr(C)` ABI, so the obvious repair — the +1
bias `nros-cpp` uses — is an ABI change, not a predicate edit.

## The precedent, which is also the evidence that it bites

`nros-cpp` had exactly this, and issue 0312 fixed it with a bias
(`packages/api/nros-cpp/src/lib.rs`, `encode_node_id`/`decode_node_id`). Its own
comment records the symptom:

> Before the bias, this was spelled `if node.node_id != 0` at eight call sites,
> and a single-node entry's `NodeId(0)` read as "no node" at every one of them.
> The visible symptom was a listener that received fine yet advertised no
> subscription to ROS 2 discovery: the arena registration fell back to the
> executor's own (empty) node name, so `create_subscription` skipped the
> liveliness token that `ros2 topic info` counts.

## Why nothing has surfaced on the C side yet — UNMEASURED

Two reasons are plausible and neither is confirmed:

* `set_executor_node_identity(rust_exec, node)` runs immediately before each of
  these, and it resolves identity from the executor's `NodeRecord`. So the
  liveliness keyexpr — the thing 0312's symptom was about — may already be
  correct on this path, which would make the `None` wrong-but-inert.
* A primary node resolves to session slot 0 anyway, so multi-RMW routing lands
  in the same place it would have.

**Do not treat either as established.** The right first step is the one 0312
took: a `ros2 topic info --verbose` / `ros2 node info` read against a live C
image whose node came from `nros_executor_node_init` — which, since issue 1384,
is an image that runs. `examples/native/c/custom-platform` is that image and now
has a runtime cell (`Workload::ExecutorBoundNode`).

## Acceptance

A measurement first, then a fix only if the measurement shows a difference:
either a peer-visible discrepancy between a primary-slot and a second-slot node
registering the same entity, or a written finding that the two are identical on
the wire and the `None` is inert — in which case the sites should still be
collapsed onto ONE decode helper with that finding as its comment, because nine
copies of a predicate that is right by accident is how 1384 happened.
