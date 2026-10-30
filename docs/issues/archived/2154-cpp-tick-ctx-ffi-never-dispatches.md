---
id: 2154
title: "A C++ tick body's service / action client calls always fail: the tick-ctx FFI is a stub the Rust side has outgrown"
status: resolved
resolved: 2026-10-30
type: bug
area: [api, cpp, executor]
severity: low
found: 2026-10-30
related: [phase-515]
---

## What

`rclcpp::TickCtx` (`packages/api/nros-cpp/include/rclcpp/tick_ctx.hpp`) gives a
C++ component's `tick` body typed `call<Req, Resp>()` and `send_goal<G>()`
wrappers. Both forward to the C-ABI pair in
`packages/api/nros-cpp/src/tick_ctx.rs`:

- `nros_cpp_tick_ctx_call_raw`
- `nros_cpp_tick_ctx_send_goal_raw`

Each validates its pointers and then returns `NROS_CPP_RET_ERROR` for EVERY
well-formed call. Nothing in the C++ runtime constructs a `TickCtx` with a real
handle and hands it to a component either; the only exercise in the tree is a
compile test (`packages/api/nros-cpp/tests/compile/rx_size_bound.cpp`).

The stub was written (Phase 212.M-F.4.c) to MATCH the Rust side, which then
answered every tick-side client call with an `UnsupportedClients` stub while it
waited on a codegen-side `GenClientDispatch`. The Rust side has since gained
real dispatch: `RuntimeClientDispatch` (`packages/api/nros/src/node_runtime.rs`)
implements `ClientDispatch`, and `ExecutorNodeRuntime::run_ticks` hands it to
every component's `tick`, beside `RuntimeActions` for the server half (which
the Linux Rust `action-server` example's tick uses to complete goals and
publish feedback). The C++ half never followed, so the
two surfaces now disagree: the same tick-body client call succeeds in Rust and
fails in C++, with `ErrorCode::Error` and nothing naming why.

The comments claiming the two stubs matched were corrected by the phase-515
W0.d prose pass (#2855); this issue is the behaviour.

## Options

- Route the C++ calls to the runtime's `ClientDispatch` the way the Rust tick
  does, and have the C++ component runtime pass a real tick-ctx handle.
- Or withdraw `TickCtx::call` / `send_goal` from the C++ surface (a C++
  component uses its own client handles from callbacks), so a C++ user cannot
  write a tick body that compiles and can never work.

## Acceptance

A C++ tick body's `call` reaches a live service server and returns its reply,
in a runtime test; or the wrappers are gone and a compile-fail probe says what
to use instead.

## Resolution (2026-10-30)

**Withdrawn** (the second option). Routing the calls was not possible
without more: a C++ component has no `tick` hook, so nothing could ever hand a
C++ user a real tick context.

- Deleted `include/rclcpp/tick_ctx.hpp` and `src/tick_ctx.rs`, i.e.
  ~~`rclcpp::TickCtx`~~, ~~`nros_cpp_tick_ctx_call_raw`~~ and
  ~~`nros_cpp_tick_ctx_send_goal_raw`~~; `rclcpp.hpp` no longer includes it.
- `nros_cpp_ffi.h` regenerated (cbindgen); the three api-parity ledger rows
  for `cpp.TickCtx` removed.
- `tests/compile/rx_size_bound.cpp` lost its `TickCtx` parameters; its
  `Client::call` paths still cover the receive-size bound.

No compile-fail probe was added: the type no longer exists, so a C++ tick body
using it fails at the first use with an unknown-name error. A C++ component
calls services and sends goals through its own client handles.
