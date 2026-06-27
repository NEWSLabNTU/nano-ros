---
id: 104
title: "C `create_node` nodes never appear in `ros2 node list` — no liveliness token"
status: open
type: bug
area: rmw
related: [phase-266, rfc-0019]
---

## Summary

Nodes created from C via `nros_cpp_node_create(executor, name, ns, &node)` are struct-only —
they do not declare a zenoh/RMW **node liveliness token**, so they never appear in `ros2 node
list` (or `ros2 node info`), regardless of name. The C entry's primary *session* IS now named
correctly (phase-266 W5 names it from `.nros_boot_config`), but the per-node C nodes have no
graph presence at all. C++ nodes via `nros::create_node` / the typed component path declare
liveliness and DO appear.

## Evidence

Found during phase-266 W5/W6 (2026-06-27): after threading the launch node name into the C
session, a native **C++** workspace entry correctly shows its node in `ros2 node list`
(`/talker`), but the analogous **C** entry's `nros_cpp_node_create` nodes do not show. The
C-side seam is documented as struct-only (no liveliness) — see the `nros_cpp_node_create` path
and the W5b/W6 implementer note ("`nros_cpp_node_create` is struct-only, no liveliness token").

## Impact

- C nodes are invisible to ROS 2 graph introspection (`ros2 node list/info`, rqt_graph).
  Topics/services still work (those declare their own liveliness via pub/sub creation), but the
  *node* entry is missing.
- This is orthogonal to naming (phase-266 #98/#101): even with the correct session name, the C
  per-node graph entry is absent.

## Fix direction

Have the C `create_node` path declare the same node-liveliness token the C++/Rust node-creation
path declares (the rmw_zenoh node admin/liveliness keyexpr keyed on domain + namespace + node
name). Likely a small addition in the C node-create FFI (`nros-c` / `nros-cpp` `nros_cpp_node_create`)
to call the existing node-liveliness declaration that the Rust/C++ node path already uses. Verify
with `ros2 node list` against a native C entry.

## Notes

Separated out of #101 (boot-config unification) so that issue can close on its node-naming scope.
This is a graph-visibility defect, not a naming one.
