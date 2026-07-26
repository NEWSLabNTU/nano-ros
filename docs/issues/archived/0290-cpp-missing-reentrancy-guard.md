---
id: 290
title: "nros-cpp blocking helpers have no reentrancy guard — calling them from a callback aliases &mut Executor (C guards it, C++ does not)"
status: resolved
type: bug
area: nros-cpp
related: [0278]
resolved_in: "issue-0290 (DispatchGuard in nros-cpp)"
---

## Finding (#278 investigation, 2026-07-26)

The C and C++ APIs disagreed about whether a blocking helper may run inside a
callback, and only C was right.

**C guards it.** `nros_executor_t` carries an `in_dispatch` flag
(`nros-c/src/executor.rs:205`) set around callback dispatch
(`executor.rs:1880-1882`). Every blocking helper checks it and refuses:

- `nros_client_call` → `NROS_RET_REENTRANT` (`service.rs:1573`)
- `nros_action_send_goal` / result helpers (`action/client.rs:337,461,578`)

There is a unit test that fakes an executor with `in_dispatch = true` and
asserts the refusal (`service.rs:2557`).

**C++ did not.** `in_dispatch` had zero uses anywhere in `nros-cpp`.
`nros_cpp_spin_once` (`nros-cpp/src/lib.rs`) unconditionally did

```rust
let ctx = unsafe { &mut *(handle as *mut CppContext) };
ctx.executor.spin_once(...)
```

so calling `Client::call()` or `Future::wait()` from inside a timer or
subscription callback re-entered `spin_once` while the OUTER dispatch still
held `&mut Executor` — two live mutable references to the same executor.
Nothing rejected it, nothing returned an error, and it compiled cleanly.

Two more C++ helpers had the same hole and bypassed the FFI entry point
entirely, spinning `ctx.executor` directly in their own loops:
`nros_cpp_action_client_send_goal` and `nros_cpp_action_client_get_result`
(`nros-cpp/src/action.rs`).

Aggravating detail: `ErrorCode::Reentrant = -15` was already declared in
`nros-cpp/include/nros/result.hpp:63` and static-asserted to match the C
value, so the error looked supported. But `nros-cpp` only ever produced it by
mapping `NodeError::RequestInFlight` — an unrelated condition. The
dispatch-reentrancy case never yielded it.

This is why the autoware-safety-island port had to weaken `requestMrmBehavior`
to send-and-poll (issue 0278 / porting-notes 14): a bounded `future.wait_for`
inside a timer callback was not merely unsupported, it was unsound.

## Resolution (2026-07-26)

Added a `DispatchGuard` RAII guard plus an `in_dispatch: bool` on `CppContext`,
and applied it at all three spin sites:

- `nros_cpp_spin_once` — refuses re-entry with `NROS_CPP_RET_REENTRANT`. This
  is the choke point for `Future::wait` and `Client::call`, which loop on it.
  `Future::wait` already treats any non-transient return as fatal and
  propagates it, so the caller now gets `ErrorCode::Reentrant` instead of
  corruption — no change needed on the C++ side.
- `nros_cpp_action_client_send_goal` and `..._get_result` — check at entry and
  restore the caller's original callback before returning `REENTRANT`, so a
  refusal cannot strand the client on the internal blocking callback.

Guarding the spin sites rather than each helper covers the whole family at
once, including any blocking helper added later.

Design notes:

- The guard borrows only the FLAG, not the whole context; call sites
  split-borrow (`let CppContext { executor, in_dispatch, .. } = ctx`). That
  keeps it testable with a `&mut bool` instead of a live `Executor`.
- It is deliberately NOT gated on `rmw-cffi`. The guard is pure flag logic
  with no FFI dependency, and the `rmw-cffi` lib-test target does not link on
  the host (undefined `nros_platform_sleep_ms`). Gating it would have produced
  a test that never runs.
- RAII rather than manual set/clear: an early return inside a spin must not
  leave the flag stuck, which would deadlock every later blocking call.

Tests (`dispatch_guard_tests`, run in the default lane): entry sets and drop
clears; a refused entry does NOT clear the outer flag (the outer spin is still
live); sequential non-nested entries each succeed, so the flag is not sticky.

## Not covered

The guard makes the in-callback case return a clean error. It does not make
the operation WORK from a callback — that needs the nested-executor or
callback-continuation design tracked in [0278](0278-no-polling-subscriber-blocking-futures.md).
