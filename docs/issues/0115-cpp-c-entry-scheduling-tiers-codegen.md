---
id: 115
title: "C / C++ entries don't wire `[tiers]` scheduling — tier resolution + `run_tiers` are Rust `nros::main!`-only, blocking Track-B ws-realtime for C/C++"
status: open
type: enhancement
area: core
related: [phase-263, phase-264, 112, 113, 114]
---

## Summary

Phase-263 Track-B `ws-realtime` (scheduling tiers, RFC-0015) is DONE for Rust
(`ws-realtime-rust`): `system.toml [tiers.*]` + per-node `callback_groups` resolve to a multi-tier
`<Board>::run_tiers(...)` entry. Projecting to **C / C++** is BLOCKED — tier resolution + the
`run_tiers` emission live entirely in the Rust `nros::main!` proc-macro; the shared entry IR + the
C/C++ emitters have no tier surface.

## Findings (file:line)

- **Tiers resolve + emit inside the Rust macro only.** `packages/core/nros-macros/src/main_macro.rs`
  reads per-node `callback_groups` (`read_node_callback_groups`, ~line 1947) + `[tiers.*]`
  (`read_system_tier_config`, ~line 628), `resolve_tiers` (~line 657), and emits
  `<Board>::run_tiers(...)` (lines 849-866). Single-tier falls back to `run`.
- **The shared entry IR has no tiers field.** `Plan` (`codegen/entry/mod.rs:86`) + `PlanNode`
  (line 207) carry nodes/host/qos_overrides only — no tier / callback-group / priority.
- **`emit_c.rs` / `emit_cpp.rs` emit no tier wiring** (grep finds none) — both emit the single-tier
  `run_components` path.
- **The planner punts to codegen, but only the macro implements it.**
  `orchestration/planner.rs:721` ("the planner emits no scheduling tiers: tiers resolve in the
  codegen tools from `system.toml` + node `callback_groups`") — only the Rust macro does that
  resolution; the C/C++ `generate`/`bake` tools do not.
- **No C component tier-declaration surface.** `nano_ros_node_register` has no
  `callback_groups`/tier parameter, so a C node cannot map itself to a tier.

## Impact

A faithful C/C++ `run_tiers` workspace is impossible without core work. Same class as 0112 (params),
0113 (lifecycle), 0114 (subscription integrity): a Rust executor/macro surface with no C/C++
entry/component projection.

## Proposed direction

- Add a `tiers` field to the entry `Plan` IR + tier resolution to `emit_c.rs` / `emit_cpp.rs`
  (emit `<Board>::run_tiers` for the C/C++ native board), and a callback-group / tier metadata
  surface on `nano_ros_node_register`. Then build `ws-realtime-{c,cpp}` + the multi-tier runtime
  test.
