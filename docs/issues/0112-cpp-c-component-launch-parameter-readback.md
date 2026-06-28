---
id: 112
title: "C / C++ components have no launch-parameter readback — the component-install seam carries no param context, blocking A2 params for C/C++/mixed"
status: open
type: enhancement
area: core
related: [phase-263, phase-264]
---

## Summary

Phase-263 A2 (parameters) is DONE for Rust (`ws-params-rust`): a Node pkg reads the launch-baked
initial via `ctx.param("name")` and re-reads the **live** value each tick via
`ctx.parameter::<T>("name")` (phase-264 W4). Projecting A2 to **C / C++ / mixed** is BLOCKED — the
C/C++ component shape has no equivalent surface:

1. **The component-install seam carries no parameter context.** A typed C/C++ component is
   constructed + `configure(node, executor, self)`-d by `<Board>::run_components`; that seam passes
   only the FFI node + executor. There is no `ctx.param` / `NodeContext` channel, so a launch
   `<param name=… value=…/>` cannot reach a C/C++ component the way `nros::main!` seeds it into the
   Rust `NodeContext`.
2. **The C/C++ entry codegen does not bake/wire launch `<param>`.** `emit_c.rs` / `emit_cpp.rs`
   contain no parameter handling (no `<param>` → baked initial, no `[param_services]` registration),
   unlike the Rust `nros::main!` W4a/W4b path.
3. **No component-side live param-read.** `<nros/component.h>` / `component.hpp` expose no
   `ctx.parameter::<T>` equivalent. The C `nros_param_server_t` (`<nros/parameter.h>`) is a
   node-local declare/get/set store, but a component is not handed one, and nothing wires it to the
   ROS 2 param services from a launch `<param>`.

## Impact

A2 params cannot be faithfully projected to C / C++ / mixed without faking the demo (the phase-263
guardrail forbids that). The other Track-A features project cleanly (A1 services, A5 logging DONE;
A4 actions has the `nros_cpp_action_*` component seams; A3 lifecycle TBD).

## Proposed direction

- Extend the component-install seam to carry a parameter context (the baked launch initials +
  optionally a live `nros_param_server_t` handle), mirroring the Rust `NodeContext` / `CallbackCtx`.
- Teach `emit_c.rs` / `emit_cpp.rs` to bake launch `<param>` initials and (when `[param_services]`)
  register the ROS 2 param services + a volatile store, mirroring W4a/W4b.
- Add a `ctx.parameter`-equivalent accessor to the C/C++ component surface.

Until then, A2 for C/C++/mixed is parked; the projection waves proceed with A4 actions next.
