---
id: 219
title: "the nros CLI is resolved by 4 divergent cmake implementations — one searches stale ~/.nros/bin BEFORE PATH (the exact shadowing its sibling's comment warns against)"
status: open
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
