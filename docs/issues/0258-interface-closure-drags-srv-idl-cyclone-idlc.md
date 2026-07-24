---
id: 258
title: "Full-pkg interface closure drags srv files whose generated IDL the embedded cyclone idlc rejects (nav_msgs GetMap et al)"
status: open
type: bug
severity: medium
area: codegen
---

## Finding (autoware-safety-island-example P3, 2026-07-24)

Declaring `<depend>nav_msgs</depend>` (for `Odometry` alone) pulls the WHOLE
AMENT package into the cyclone typesupport stage, including its srv files —
and the generated `GetMap_Response.idl` / `LoadMap.idl` / `SetMap.idl` fail
cyclone's idlc:

```
_idlroot/nav_msgs/msg/GetMap_Response.idl:15: error: Can't open include file "std_msgs/msg/Header.idl"
... 1 error in preprocessor / syntax error
```

Workaround shipped in the example: a workspace-shadowing `nav_msgs` subset
pkg (Odometry only) — shadowing works (Phase 210 fixture), but every consumer
of a big upstream msg pkg will trip this.

## Fix directions

Either scope the cyclone-ts generation to the msg types actually reachable
from the consumer's used set (the resolver knows the closure), or fix the
srv→IDL lowering (missing include-path emit for cross-pkg includes inside
srv-derived IDL) so full packages compile.
