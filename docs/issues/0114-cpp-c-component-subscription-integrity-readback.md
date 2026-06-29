---
id: 114
title: "C / C++ executor-component subscriptions have no E2E-integrity readback — only the imperative `try_recv_validated` poll path does, blocking Track-B ws-safety for C/C++"
status: open
type: enhancement
area: core
related: [phase-263, phase-252, 73, 112, 113]
---

## Summary

Phase-263 Track-B `ws-safety` (E2E safety / CRC) is DONE for Rust (`ws-safety-rust`): a publisher
attaches the E2E CRC (rides the backend `safety-e2e` feature), and a managed subscriber VALIDATES
it via `CallbackCtx::integrity()`. Projecting to **C / C++** is BLOCKED on the validate side — the
C/C++ executor-**component** subscription has no integrity-carrying callback.

## Findings (file:line)

- **Publisher CRC-attach — FEASIBLE (no code).** Automatic via the backend `safety-e2e` feature
  (`packages/core/nros-c/Cargo.toml:34-39`); a C talker component attaches the CRC just by being
  built with it. The build knob is reachable from a workspace: `[system].features = ["safety"]` →
  `nros codegen-system` (`cmd/codegen_system.rs:396`) → `nros_lower_system_features` →
  `NANO_ROS_SAFETY_E2E=ON` (`cmake/NanoRosCapabilities.cmake:60-66`).
- **Subscriber VALIDATE — BLOCKED.** The CRC-validate / integrity surface exists ONLY on the
  imperative polling subscription: C `nros_subscription_try_recv_validated(...,
  nros_integrity_status_t*)` (`nros_generated.h:5676`, used by the standalone
  `examples/native/c/safety-listener/src/main.c:60` under `NROS_APP_MAIN_REGISTER_POSIX` — a
  standalone binary, NOT a component); C++ `nros_cpp_subscription_try_recv_validated(void* storage,
  …)` (`nros_cpp_ffi.h:1168`), same poll-on-created-subscription shape.
- The **executor-driven component callback** — the only subscription a workspace Node pkg can
  register — is fixed at `(data, len, ctx)` with NO integrity arg and no `_validated` variant:
  `nros_c_subscription_callback_t` (`component.h:77`), registered via
  `nros_cpp_subscription_register` (`component.h:125`). No integrity-carrying component register
  exists.

## Why Rust works (the A2/A3 pattern, third instance)

Rust threads integrity through the executor callback context —
`create_subscription_..._with_safety` → `FnMut(&[u8], &IntegrityStatus)`
(`nros-node/src/executor/node.rs:1484-1498`) → `CallbackCtx::integrity()`
(`ws-safety-rust/.../safe_listener_pkg`). The C/C++ **component-callback projection** of that path
was never built (issue 0073 / phase-252 added only the imperative-poll `try_recv_validated`). So
the validate side has no path into the `NROS_C_COMPONENT` / launch / `run_components` model a
workspace requires.

## Impact

`ws-safety-c` and `ws-safety-cpp` are blocked on the SAME missing surface (one fix unblocks both).
Same class as 0112 (params) + 0113 (lifecycle): a Rust executor/component surface with no C/C++
component projection.

## Proposed direction

Add an integrity-aware subscription to the C/C++ executor-component model — e.g.
`nros_cpp_subscription_register_validated(...)` whose callback carries `nros_integrity_status_t`
(the C/C++ analog of Rust `create_subscription_..._with_safety` + `CallbackCtx::integrity()`), or a
component-pollable validated subscription drainable on the executor. Then build `ws-safety-{c,cpp}`
+ a cross-process e2e asserting the listener validates the CRC (and catches a corrupted frame).
