---
id: 219
title: "the nros CLI is resolved by 4 divergent cmake implementations — one searches stale ~/.nros/bin BEFORE PATH (the exact shadowing its sibling's comment warns against)"
status: resolved
type: bug
severity: medium
area: build
related: []
---

## Findings (deep audit 2026-07-17, A5/I1)

- `cmake/nano_ros_workspace_metadata.cmake:24` — `find_program(... HINTS ENV
  NROS_CLI ENV NROS_HOME PATHS "$ENV{HOME}/.nros/bin")`: HINTS are searched
  BEFORE the environment PATH, so a stale provisioned `~/.nros/bin/nros`
  shadows the activate.sh-wired in-tree CLI — the precedence bug
  `zephyr/cmake/nros_system_generate.cmake`'s own comment explicitly warns
  against. It also passes raw `ENV NROS_HOME` (no `/bin` suffix), unlike
  every sibling.
- Four independent resolvers exist with subtly different precedence:
  `NanoRosCodegenCore.cmake` `_nros_resolve_codegen_tool` (the documented
  SSoT), `NanoRosEntry.cmake` `_nros_entry_invoke_codegen`,
  `nano_ros_workspace_metadata.cmake` inline, and
  `zephyr/cmake/nros_system_generate.cmake` `_nros_system_resolve_cli` —
  despite a Phase-246.2b comment claiming the shared core owns this.

## Fix sketch

Extend the NanoRosCodegenCore resolver to serve the plain-CLI case and route
all four call sites through it; one documented precedence order (PATH-wired
in-tree CLI first, provisioned store as PATHS fallback).

## Resolution (2026-07-17)

One shared `nros_resolve_cli(<out> [CONTEXT] [OPTIONAL])` now lives in
NanoRosCodegenCore.cmake with the documented precedence: `$NROS_CLI` env
override > already-resolved codegen-tool cache vars > find_program with the
environment PATH first and the provisioned store (`$NROS_HOME/bin`,
`~/.nros/bin`) strictly as PATHS fallbacks (never HINTS), plus
stale-cache-drop. All four call sites rewired: `_nros_resolve_codegen_tool`
delegates its search (keeps its richer FATAL text), NanoRosEntry keeps only
its `NROS_CLI_BIN` cache override, nano_ros_workspace_metadata keeps only
`NROS_BIN`, and the zephyr `_nros_system_resolve_cli` is a thin shim.

Verified: ws-custom-msg-c configures + builds through the new resolver
(in-tree CLI found via PATH); direct precedence tests — a fake stale
`$NROS_HOME/bin/nros` does NOT shadow the PATH-wired CLI (the reported
bug), `$NROS_CLI` overrides everything, and the store fallback engages when
PATH lacks nros. nano_ros_workspace_metadata.cmake stays under its 150-LoC
budget (131).
