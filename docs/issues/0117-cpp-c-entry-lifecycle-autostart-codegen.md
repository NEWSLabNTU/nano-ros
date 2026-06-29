---
id: 117
title: "C / C++ entries don't wire `[lifecycle]` autostart — the entry codegen + native board never register the lifecycle services or drive Configure→Activate, blocking A3 lifecycle for C/C++/mixed"
status: open
type: enhancement
area: core
related: [phase-263, phase-264, 116]
---

## Summary

Phase-263 A3 (lifecycle) is DONE for Rust (`ws-lifecycle-rust`): `[lifecycle] autostart = "active"`
in `system.toml` + the `nros/lifecycle-services` feature; `nros::main!` emits
`runtime.apply_lifecycle(code)`, which registers the 5 REP-2002 services + drives Configure→Activate
at boot (the `ros2 lifecycle get → active` interop test). Projecting A3 to **C / C++ / mixed** is
BLOCKED — the C FFI has the lifecycle *primitives* but nothing threads `[lifecycle]` into a C/C++
entry.

## Findings (file:line)

1. **The C/C++ lifecycle FFI EXISTS but is unused.** `packages/core/nros-c/src/lifecycle.rs`
   (+ `include/nros/lifecycle.h`) exposes the executor-integrated surface —
   `nros_executor_register_lifecycle_services`, `nros_executor_lifecycle_change_state`,
   `..._get_state`, `..._register_on_*` (`lifecycle.rs:296-451`, gated on
   `lifecycle-services` + `rmw-cffi`). A C entry *could* register the services + step transitions,
   but nothing calls these.

2. **The entry `Plan` drops `[lifecycle]`.** The planner reads `[lifecycle]` into `plan.lifecycle`
   (`orchestration/planner.rs:760-765`), but the entry `Plan` struct
   (`codegen/entry/mod.rs:86`) has **no `lifecycle` field**, so it never reaches the C/C++ emitters.

3. **`emit_c.rs` / `emit_cpp.rs` emit no lifecycle wiring.** The generated `__nros_entry_setup`
   does init → create-node → configure-component → `nros_board_native_run_components_named`, with no
   `nros_executor_register_lifecycle_services()` call, no autostart Configure→Activate, no
   `lifecycle-services` feature/link request.

4. **The native board has no autostart.** `nros_board_native_run_components_named`
   (`packages/core/nros-cpp/src/lib.rs:565`) does init → setup → spin → fini; it never registers the
   lifecycle services or drives transitions. The autostart driver
   (`RuntimeCtx::apply_lifecycle`, `nros-platform/src/board/runtime.rs:264`) is reachable ONLY via
   the Rust `nros::main!` macro path (`main_macro.rs:725-733`).

## Impact

A3 lifecycle cannot be faithfully projected to C / C++ / mixed without faking it (hand-writing
lifecycle calls into a non-generated entry — the phase-263 guardrail forbids that). Sibling of
issue 0116 (A2 params, same class: a Rust macro/runtime surface the C/C++ entry path lacks).

## Proposed direction

- Thread `plan.lifecycle` into the entry `Plan` (codegen/entry/mod.rs) so the C/C++ emitters see it.
- Have `emit_c.rs` / `emit_cpp.rs` emit `nros_executor_register_lifecycle_services(executor)` in
  `__nros_entry_setup` + an autostart Configure→Activate drive (or add an `autostart` arg to
  `nros_board_native_run_components_named` that calls register + `change_state`), and enable the
  `lifecycle-services` feature on the generated entry's link.
- Then build `ws-lifecycle-c` + the `ros2 lifecycle get → active` interop test.

Until then, A3 for C/C++/mixed is parked; the projection waves proceed with Track B advanced
workspaces next.
