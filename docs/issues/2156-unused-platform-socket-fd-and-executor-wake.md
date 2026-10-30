---
id: 2156
title: "Every zenoh message on a Linux image signals a condvar nothing waits on; two public helpers have no caller"
status: open
type: bug
area: [rmw, zenoh, platform]
severity: low
found: 2026-10-30
related: [phase-515]
---

## What

Two helpers outlived their only consumer. One of them still costs something on
the receive path.

### `signal_executor_wake` / `wait_for_executor_wake` (zenoh shim, `std`)

`packages/rmw/zenoh/nros-rmw-zenoh/src/shim/mod.rs` keeps a process-global
`EXECUTOR_WAKE: LazyLock<(Mutex<bool>, Condvar)>`. Its waiter,
`wait_for_executor_wake`, was called by `BasicExecutor::spin()`, which is gone;
nothing in the tree calls it now (it stays a `pub use` in `lib.rs`). The
executor parks on its own primitive (`NodeWake`, woken through the session's
`set_wake_callback`), not on this condvar.

The SIGNALLER is still live. `signal_executor_wake()` runs from
`subscriber_notify_callback` (`shim/subscriber.rs`) and `queryable_callback`
(`shim/service.rs`), i.e. on the zenoh-pico read thread for every arriving
sample and query, and each call takes the mutex and `notify_one`s a condvar
with no waiter. It is compiled under `nros-rmw-zenoh/std`, which
`nros-board-linux` enables, so every Linux Rust image on zenoh pays it.

### `nros_platform_socket_get_fd` (platform ABI header)

`packages/platform/nros-platform-api/include/nros/platform_net.h` defines it
`static inline` (Phase 154) for zpico-sys's `get_session_fd`, on the ThreadX-Linux
read-task wakeup path. That helper is gone and nothing calls the accessor; it
survives in two build-descriptor comments
(`packages/rmw/zenoh/nros-rmw-zenoh/nros-rmw.toml`) and as the example in
`scripts/check-platform-abi-mirror.sh`'s exemption note.

Both were found by the phase-515 W0.d prose pass (#2852), which corrected the
comments that still named their callers.

## Fix

- Delete `EXECUTOR_WAKE`, `wait_for_executor_wake`, `signal_executor_wake`,
  their two call sites and the `pub use` — or, if an out-of-tree user is meant
  to wait on it, say so and keep only the waiter's half alive on purpose. Measure
  the receive path before and after; the claim here is "unneeded work", not a
  number.
- Delete `nros_platform_socket_get_fd` and the comments naming it, or record
  why the platform ABI should keep an accessor nobody calls.
