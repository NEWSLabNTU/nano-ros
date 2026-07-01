---
id: 124
title: "rclcpp-shape C++ components aren't bound to a scheduling tier — the entry can't thread a sched-context into an IS-A-node constructor"
status: open
type: enhancement
area: core
related: [phase-269, 119, rfc-0044, rfc-0015, rfc-0047, phase-272]
---

> **Design (2026-07-01).** Rather than a narrow per-shape patch (add `sc_id` to `NodeHandle`), this
> is folded into the unified, config-driven binding of **[RFC-0047](../design/0047-unified-sched-context-binding.md)**
> / **phase-272**: a config-seeded `node_name → sched_context` table looked up at the one
> `Executor::node_builder(name)` site every node funnels through (RFC-0046). rclcpp-shape nodes call
> `node_builder(name)` in their ctor (via `ComponentNode`→`Node::create`), so they get their tier by
> name automatically — no `NodeHandle` change. Resolved when phase-272 W3 lands the rclcpp realtime
> e2e.

## Summary

Phase-269 W4 (#119) wired `[tiers]` scheduling into the C/C++ entry: the entry codegen resolves each
node's tier to a `sched_context` and binds it so the node's entities run on that tier. This works for
**C nodes** and **configure-shape C++ nodes** but NOT for **rclcpp-shape** (RFC-0044,
IS-A-node / construct-with-handle) C++ components — they silently land on the default (untiered)
scheduling context.

## Why (file:line)

The covered path creates the node through a builder the entry controls, attaching the tier before
build (`emit_cpp.rs:~429`):

```cpp
::nros::NodeBuilder(::nros::global_handle(), "name").sched(__nros_sc_ids[idx]).build(__nros_node_i);
```

`.sched()` attaches the sched-context before `.build()`, so every entity the node creates lands on
that tier.

An **rclcpp-shape** component OWNS its node and constructs it **inside its own constructor**; the
entry only placement-news the component with a bare handle (`emit_cpp.rs:~406-412`):

```cpp
::nros::NodeHandle __h(::nros::global_handle());   // carries only the *mut Executor
__nros_comp_i = new (__nros_comp_buf_i) ::Cls(__h);
```

The node + entities are created inside `Cls`'s ctor — past the point the entry controls — and
`NodeHandle` (`nros-cpp/include/nros/node.hpp`) carries only the executor pointer, with **no
`sc_id` field** to pass the resolved tier through. So the entry cannot bind an rclcpp-shape node to a
tier. `is_rclcpp_node` nodes are skipped in the sched-binding branch (the `NodeBuilder::sched` arm is
configure/C-shape only).

## Impact

A realtime (`[tiers]`) workspace whose components use the rclcpp-shape (RFC-0044, IS-A-node) style
gets no per-tier scheduling for those nodes — they run on the default context. The configure-shape
(RFC-0043 default) + C paths ARE covered, so the common case works; only the IS-A-node style is
affected. Low urgency (no current realtime fixture uses rclcpp-shape).

## Fix direction

Thread a sched-context id into the rclcpp construct-with-handle path: add an `sc_id` to `NodeHandle`
(e.g. `NodeHandle(global_handle(), sc_id)`), have the entry pass the resolved
`__nros_sc_ids[idx]` when placement-newing an rclcpp-shape component, and have the component's
internal node creation honor it (bind on the handle before the node's entities are created). A small
`NodeHandle` ABI addition + one emit branch + the component-side node-create honoring the handle's
sched id. Then add an rclcpp-shape `ws-realtime` variant + assert per-tier scheduling.
