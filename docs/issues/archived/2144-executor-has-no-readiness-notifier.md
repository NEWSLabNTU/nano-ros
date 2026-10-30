---
id: 2144
title: "No executor-level readiness notifier: a host event loop cannot learn that work arrived, so it must poll"
status: resolved
resolved: 2026-10-30
type: enhancement
area: [core, executor, rmw]
severity: medium
found: 2026-10-29
related: [2143, 2145, 2146, 2162]
---

## What

RFC-0112 D2's Tier 0 contract needs a **Readiness out** part: when data,
a request or a reply arrives, the executor tells the host, so the host
schedules a spin instead of polling.

There is none at the executor level. What exists:

- **Per-entity wakers.** The RMW traits have `register_waker`
  (`packages/core/nros-rmw/src/traits.rs:3750`, `:3977`, `:4424`; the defaults
  return `false`). The C polling API wraps them as
  `nros_*_set_wake_callback` / `rclc_polling_*_set_*_wake_callback` through
  `packages/core/nros-node/src/c_waker.rs`. A host would have to register one
  per entity and track them itself.
- **Inbound wakes.** `wake()` (`spin.rs:16526`) and `wake_handle()` (`:16565`)
  wake a parked executor. They carry no information outward.
- **`signal_fd()`** (`spin.rs:6185`, Linux, `signal-fd-wake`) is a WRITE end
  for signal handlers. A GLib, Qt or epoll loop cannot watch it for
  readability.

So no Shape B host (RFC-0112 table) can integrate without busy-polling. That
covers GLib/Qt/epoll, tokio and Embassy beyond `spin_async`'s busy loop
(issue 2145), and an RTOS task that wants to block until work arrives.

## Direction (RFC-0112 D2.2)

- Add one hook, `set_ready_notifier(fn(*mut c_void), ctx)`. The executor
  arms it on every entity's `register_waker` and calls it on any wake. It is
  callable from any context, and the callback must not block or re-enter.
- On Linux, add an optional readable fd built on it for host event loops.
- Cross the C and C++ FFI in the same change.

**Open, measure first (RFC-0112 open question):** can the notifier fire in ISR
context for every backend on bare metal (zenoh-pico/smoltcp, XRCE/serial), or
must it be "set a flag and pend an interrupt"? And for backends whose
`register_waker` returns `false`, the fallback must be a refusal or a stated
polling period, never a notifier that silently never fires.

## Resolution

Landed as RFC-0112 D2.2 describes, built on the executor's wake context
(`WakeCtx`) rather than on a second per-entity registration:

- **`Executor::set_ready_notifier(Option<fn>, ctx)`** (`unsafe`: the context
  contract). `WakeCtx::notify` runs at the end of both arrival paths, the
  session-scoped `signal` (which a cross-thread guard trigger also takes) and
  the attributed entity waker's `mark_ready`, before its band routing returns.
  So every backend that wakes at all reaches it, attributed or not, whichever
  worker is woken. The `(fn, ctx)` pair is published through one pointer; a
  replaced pair is kept until the context drops, because a backend thread may
  be mid-call. The executor clears it on `close` and drop.
- **Refusal, not silence.** `ready_notifier_supported()` is the predicate the
  executor's own park trusts: a live async wake from the backend and a wake
  primitive. XRCE and uORB learn of an arrival only inside `drive_io`, so they
  get `Transport(Unsupported)`; clearing never fails.
- **`Executor::ready_fd()`** (Linux, `signal-fd-wake`): a non-blocking eventfd,
  owned by the wake context and closed with it, written at the same moments.
- **FFI:** `rclc_executor_set_ready_notifier` (C, `RCL_RET_UNSUPPORTED` on a
  poll-only backend) and `nros_cpp_executor_set_ready_notifier` /
  `rclcpp::Executor::set_ready_notifier` (C++).

**The ISR open question, answered by survey rather than by measuring a
notifier in an ISR:** no in-tree backend calls its wake from an interrupt.
zenoh-pico fires from its read task, or from `drive_io` inside the executor's
own spin (smoltcp on bare metal is polled there, never from the NIC ISR);
Cyclone from its listener threads; a guard from the triggering thread; XRCE
and uORB install none. The contract states thread context. RFC-0112's open
question is struck with that survey.

Tests (each with a negative control, measured by mutation):

- `spin.rs` `ready_notifier_tests`: `signal`, `mark_ready` and a band-routed
  `mark_ready` all notify; same-pair re-install keeps nothing; a replaced or
  cleared pair is not called. Control: `notify` dropped from `mark_ready` ->
  red ("an attributed arrival notifies").
- `nros-tests/tests/entity_wake.rs`
  `the_ready_notifier_fires_with_nothing_spinning` (zenoh): an arrival with
  nothing spinning notifies and the prompted `spin_some(ZERO)` dispatches it;
  a cross-thread guard trigger notifies; a cleared notifier is not called while
  arrivals still dispatch. Control: `notify` made a no-op -> red.
- `signal_fd_wake.rs` `ready_fd_becomes_readable_on_arrival`: readable after a
  trigger and after a publish with nothing spinning; distinct from
  `signal_fd()`. Control: the eventfd write skipped -> red.
- `xrce_guard_wake.rs` `a_poll_only_executor_refuses_a_ready_notifier`.
  Control: the predicate without `async_wake_live()` -> red.
- `nros-c/tests/run/ready_notifier.c` (`just check c`) and
  `nros-cpp/tests/compile/ready_notifier_runtime.cpp` (`just check cpp`), on
  the stub backend's own wake path.

Residue: issue 2162, found by the zenoh test. A guard condition beside an idle
subscription is never dispatched under the default `Trigger::Any`, so the test
asserts the guard's notification and not its dispatch. Issue 2145 (a
waker-driven `spin_async`) and 2143 (`next_deadline_us`) can now build on this.
