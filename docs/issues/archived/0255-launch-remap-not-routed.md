---
id: 255
title: "launch <remap> parsed but not routed; ~/ private names unsupported — ported nodes hardcode resolved topic names"
status: resolved
type: enhancement
area: codegen
---

## Finding (autoware-safety-island-example ports, 2026-07-24)

Upstream Autoware nodes declare `~/input/...` / `~/output/...` names and get
wired by launch `<remap>`. nano-ros parses remaps (`nros-launch-parser` fills
`NodeSpec.remaps`) but neither the macro arm nor the model arm routes them,
and `~` expansion does not exist — so every ported node hardcodes the
resolved contract names in-source and the launch XML remaps are
documentation only.

This is the single largest source-diff class in the ports (porting-notes 07,
every node). Routing = project remaps into entity creation at entry codegen
time (the model already carries per-node structure), plus `~` expansion
against the node name.

## Resolution (2026-07-26, phase-305 W3+W4)

One resolution seam (nros-node names.rs: ROS 2 ~/relative expansion +
exact-FQN first-match) applied at ExecutorSink::create_entity and at the
C ABI registration sites via an executor-side per-node remap table
(nros_cpp_declare_remap). Bake: model-arm node_remap_bakes populate the
previously-dead RuntimeCtx.remaps; PlanNode.remaps + both C/C++ emitters
bake declare_remap calls; group <remap> merges into member nodes.
Runtime-proven: ws-remap-rust e2e — ~/out on a /island-namespaced node
remapped to /remapped_out arrives there and NOT on the unremapped
expansion. Residuals: C/C++ runtime cell (emitter-unit-tested only),
ComponentMeta.remaps threading (TODO in cargo_metadata_schema.rs),
embedded lanes. Behavior note: relative names on ns!=/ nodes now expand
on the wire (correct ROS 2 semantics; root-ns images unchanged).
