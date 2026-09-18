---
id: 1385
title: "No C executor installs a backend wake callback, on any backend — and
  the naive fix is unsafe because `Executor::drop` never clears one"
status: open
type: bug
area: [api, core, rmw]
severity: medium
found: 2026-09-18
related: [phase-417, phase-124, issue-1384]
---

## What is true

`Executor::install_wake_signal_on_primary` hands the backend
`nros_rmw_runtime_wake_cb` plus a context pointer, so an asynchronous arrival
can cut a parked `spin_once` short. Three paths call it, all in
`packages/core/nros-node/src/executor/spin.rs`: `Executor::open`,
`open_multi` and `open_sized`'s shared `open_in`.

The C executor reaches none of them. `nros_executor_init`
(`packages/api/nros-c/src/executor.rs`) constructs through
`CExecutor::from_session_ptr_in` — the BORROWED-session constructor — and that
constructor installs nothing. So `has_async_wake` stays `false` for the entire
life of every C executor, on every backend.

It is not backend-specific and it is not a poll-only-backend story: zenoh
(`packages/rmw/zenoh/nros-rmw-zenoh/src/shim/session.rs`) and cyclonedds
(`packages/rmw/cyclonedds/nros-rmw-cyclonedds/src/vtable.cpp`) both implement
the `set_wake_callback` slot. XRCE and uORB leave it `NULL` and are unaffected
by definition. Rust images on zenoh or cyclone get the wake; C images on the
same backend do not.

## What a C caller loses

`spin_once`'s wait decision, quoting the live code:

```rust
if was_woken {
    0
} else if self.has_async_wake && let Some(wake) = self.node_wake.as_ref() {
    let _ = wake.wait_ms(timeout_ms as u32);   // woken early by the backend
    …
    0
} else {
    // No wake primitive linked, or a poll-only backend: drive the
    // transport for the full timeout.
    timeout_ms
}
```

A C executor always takes the `else`. It is not a hang — `drive_io(timeout_ms)`
blocks in the transport's own recv, so data arriving on the primary session
still returns promptly — but everything the callback exists for is gone: an
arrival signalled from a backend worker thread or an ISR rather than from the
recv we are parked in cannot shorten the wait, and the executor waits out the
caller's whole budget instead.

The cross-thread `Executor::wake()` / `cancel()` path is unaffected: those set
`wake_flag` directly and reach the `was_woken` arm without any backend
involvement. The loss is specifically the BACKEND's async wake.

For the size of that, phase-417 W4.e (PR #1064) measured the structurally
identical `was_woken` defect one arm over: 400 ms to dispatch against 50 ms for
the same trigger issued while the spin was already parked.

## Why the naive fix is unsafe, which is the interesting half

Calling `install_wake_signal_on_primary` from `nros_executor_init` would leave a
backend holding a callback into freed executor state.

The context pointer is `Arc::as_ptr(&self.wake_ctx)` — executor-owned, and freed
when the executor drops. The callback's own SAFETY comment states the obligation:

```rust
// SAFETY: ctx points at a `WakeCtx` owned by an Executor still
// alive at the time of the call. Executor::drop must clear the
// callback via `set_wake_callback(None, _)` on all sessions
// before dropping wake_ctx; this happens in `install_wake_*`
// teardown path.
```

**There is no such teardown path.** `set_wake_callback(None` appears nowhere in
the tree — the string occurs only in that comment. `impl Drop for Executor`
(spin.rs) runs the shutdown hooks, drops component cells and drops the arena
entries; it never touches the sessions' callbacks.

On the Rust paths that window is narrow and has stayed latent: the session is
`SessionStore::Owned`, so it dies inside the same `Executor::drop`. On the C
path it is wide open by construction. `nros_executor_init` takes a session
pointer borrowed from `nros_support_t`; `rclc_executor_fini` does
`drop_in_place` on the executor and then **zero-fills `_opaque`**, while the
session lives on until a later `nros_support_fini`. Between the two calls the
backend holds a callback whose context has been freed and whose storage has been
overwritten, and any arrival in that window calls it.

## Fix shape, and the order

Two coupled defects. The order is not negotiable:

1. **First**, give the executor a teardown that clears what it installed:
   `set_wake_callback(None, ptr::null_mut())` on the primary session and on
   every extra session, in `Executor::drop`, before `wake_ctx` is dropped. This
   is the obligation the existing SAFETY comment already asserts, so it is a
   bug fix on its own account even with no C change — it closes the narrow
   Rust-side window too. Note cyclonedds already clears its own callback when
   the session is destroyed (`session.cpp`, `session_set_wake_callback(session,
   nullptr, nullptr)`), which is the backend half of the same contract; the
   executor half is what is missing.
2. **Then** install from `nros_executor_init`, i.e. make
   `from_session_ptr_in`'s C caller do what the three `open*` paths do. Doing
   this first converts a missed optimisation into a use-after-free.

Acceptance for step 1 is a test that drops an executor and then drives the
session — a backend that would have called a dangling callback must call
nothing. Acceptance for step 2 is a C-side latency assertion with a LOWER bound
on the fast case, not just an upper bound (the "assert lower bounds on timing"
rule in CLAUDE.md): a trigger delivered mid-spin must dispatch well inside the
spin budget, and the same trigger must still dispatch when the budget is long.

## Not fixed in PR #1064

W4.e found this while fixing the `was_woken` arm and deliberately left it: the
wave's boundary was the wait decision, and this needs a drop-side change in
`nros-node` plus a C-side install, with the ordering constraint above.
