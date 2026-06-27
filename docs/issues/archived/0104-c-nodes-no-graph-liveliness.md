---
id: 104
title: "C entries are invisible in `ros2 node list`; node liveliness token never declared on any path"
status: resolved
type: bug
area: rmw
related: [phase-266, rfc-0019, rfc-0005]
resolved_in: "194babcf1"
---

## Resolution (2026-06-27, `194babcf1`)

The primary node now declares its ROS 2 node liveliness token at session open, so C, C++, and
Rust entries appear in `ros2 node list`. Fix: added `node_name`/`namespace`/`domain_id` to
`TransportConfig` (`nros-rmw`), threaded them from `RmwConfig` in `ZenohRmw::open`
(`nros-rmw-zenoh`), and `ZenohSession::new` now calls `declare_node_liveliness` and **holds** the
returned `LivelinessToken` for the session's lifetime (a dropped token would undeclare). Verified
over the wire: a native C entry that previously showed **empty** `ros2 node list` now shows
`/node`; Rust + C++ no regression; `just check` green.

**Residuals (separate, NOT this issue):** per-node liveliness tokens for the individual
`create_node` components (so a multi-node entry shows `/talker` + `/listener` rather than one
`/node`) → **#105**. C-side entity identity propagation (so a C node's topics associate with it in
`ros2 node info` / `ros2 topic list`) — a smaller follow-up; the node itself is now visible.

---

## Summary

Two related defects, found + verified investigating phase-266 (2026-06-27):

1. **The node-level liveliness token is never declared on ANY path (C / C++ / Rust).**
   `ZenohSession::declare_node_liveliness(domain_id, namespace, node_name)` exists
   (`packages/zpico/nros-rmw-zenoh/src/shim/session.rs:287`; keyexpr
   `@ros2_lv/<domain>/<zid>/0/0/NN/%/<ns>/<node>` at `shim/mod.rs:394`) but has **no callers**.
   `Executor::open` only *stores* the identity via `set_node_identity`
   (`nros-node/src/executor/spin.rs:127`, `:1272`); it never declares the token. So a node
   appears in `ros2 node list` ONLY as a side effect of **entity** liveliness — i.e. because a
   publisher/subscriber it creates carries the node name in its keyexpr. An entity-less node is
   invisible.

2. **C entries are entirely invisible — even nodes that DO publish.** A native C workspace entry
   (running, actively publishing) shows **nothing** in `ros2 node list` — not even the primary
   node. The C entity-creation path doesn't propagate node identity to its entity liveliness, so
   no usable liveliness reaches the graph (`nros-c/src/executor.rs:73-77`: "without identity, no
   liveliness token is declared and rmw_zenoh subscribers won't discover the entity"). C++ entries
   show their node because their entities declare liveliness WITH the name.

## Evidence

- Native **C++** single-node entry → `ros2 node list` = `/talker` (phase-266 W6, verified).
- Native **C** entry (talker+listener, publishing — `[c_talker_pkg] sent: N` in its log) →
  `ros2 node list` = **empty** (verified 2026-06-27, same zenohd + pinned rmw_zenoh harness).
- `declare_node_liveliness` defined at `session.rs:287`, zero call sites across the tree.
- `RmwConfig` carries `node_name`/`namespace` (`nros-rmw/src/traits.rs:788`) but
  `TransportConfig` (`nros-rmw-zenoh/src/shim/transport.rs:56`) drops them, so the backend never
  has what `declare_node_liveliness` needs.

## Impact

- **C users: nodes are undiscoverable** in `ros2 node list` / `ros2 node info` / rqt_graph
  (topics still route via entity liveliness, but graph introspection shows nothing). Significant.
- **All languages:** an entity-less node (lifecycle-only, timer-only before first publish) has no
  graph presence, because node liveliness is never declared independently of entities.

## Fix direction

Declare the node liveliness token explicitly, the proper way:
- Thread `node_name`/`namespace` from `RmwConfig` → `TransportConfig` → the zenoh session
  (`transport.rs:56`), then call `ZenohSession::declare_node_liveliness(domain, ns, name)` once
  the primary session opens (in `ZenohRmw::open` or right after `Executor::open`'s
  `set_node_identity`, `spin.rs:~121`). This makes the primary node discoverable for ALL
  languages, entity-less included.
- For secondary nodes (`create_node` reusing the primary — see #105), declare a node-liveliness
  token per distinct node once per-node sessions/identity land.
- Separately ensure the **C** entity-creation path propagates node identity so C entities declare
  liveliness (the `nros-c/executor.rs` propagation the comment describes) — needed even before the
  node-token fix, since today C is invisible.

Verify: native C entry shows `/talker` in `ros2 node list`; a publish-less Rust/C++ node also
appears.

## Notes

Split from #101 (boot-config naming) — this is graph *visibility*, not naming. The original
framing ("C `create_node` struct-only") was too narrow: the node-liveliness token is missing
everywhere, and C is invisible entirely (not just per-node).
