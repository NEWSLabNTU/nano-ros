---
id: 255
title: "launch <remap> parsed but not routed; ~/ private names unsupported — ported nodes hardcode resolved topic names"
status: open
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
