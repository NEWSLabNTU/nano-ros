---
id: 105
title: "Multi-node entry collapses to one graph node — per-node naming needs per-node sessions"
status: open
type: enhancement
area: core
related: [phase-266, rfc-0045, rfc-0046, rfc-0004]
---

> **Design (2026-06-28).** Studied: one zenoh session CAN host N graph nodes — the NN liveliness
> keyexpr (`@ros2_lv/<domain>/<zid>/0/0/NN/%/<ns>/<node>`, `mod.rs:410`) identifies a node by its
> **name**, not the session id (`0/0` is protocol version, not a node-id slot). So the fix is
> **per-node liveliness tokens on the shared session**, NOT a session per node. Two halves:
> - **Graph half (this issue):** add `node_liveliness: Option<LivelinessToken>` to `NodeRecord`;
>   in `NodeBuilder::build()` declare a token per `create_node` (token held for the node's life —
>   a dropped token undeclares). Plus: gate the #104 primary `/node` token off when ≥1 named
>   component exists (else a 2-node launch shows `/node` + `/talker` + `/listener`).
> - **Naming half ([RFC-0046](../design/0046-launch-authoritative-node-identity.md)):** the per-node
>   name is **launch-authoritative** (`<node name= namespace=>` overrides the code default),
>   resolved at the shared `Executor::node_builder` site, uniform across Rust/C/C++. Stops Rust
>   hardcoding `create_node` names.
> Together → a multi-node entry shows its components named from the launch, uniformly.

## Summary

An entry that launches **N components on one `Executor`** shows a SINGLE node in `ros2 node
list` (the primary session's name), not one node per component. `create_node("talker")` /
`create_node("listener")` reuse the primary session (NodeId 0) when their rmw+locator match
(`nros-node/src/executor/node_record.rs:228`), so each component's name is recorded but the
graph liveliness keeps the primary session's single name. Same behavior for Rust and C/C++.

## Evidence

phase-266 (2026-06-27): after the boot-config unification, a single-node launch correctly names
the graph node (Rust `/param_talker`, C++ `/talker`). But a two-node C++ launch (`talker` +
`listener`) shows only `/node` (the unified primary-session default) — both components collapse
onto the one primary session. This is the deferred half of #98/#101: those issues resolved
single-node naming and explicitly deferred the multi-node case.

## Impact

- Multi-component entries are under-represented in the ROS 2 graph: one node where the launch
  declared several. Topics still route correctly (per-entity), but `ros2 node list` / per-node
  introspection don't reflect the declared topology.
- Per-node parameters / per-node namespaces are likewise scoped to the single primary node today
  (the W4c param-store note in phase-264 flagged this).

## Fix direction

Give each launch component its own node identity in the graph — either a session per node, or
(lighter) a per-node liveliness token under the shared session keyed on the component name +
namespace, so `node_record` stops collapsing distinct `create_node` calls onto NodeId 0. Decide
alongside per-node parameter-store scoping (phase-264 W4c deferral). Applies uniformly to Rust
and the C/C++ blob path (the names are already baked per component; only the graph declaration
collapses).

## Notes

Split from #98 / #101 so those close on their single-node scope. This is the multi-node
enhancement they both deferred.
