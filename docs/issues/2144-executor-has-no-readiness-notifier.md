---
id: 2144
title: "No executor-level readiness notifier: a host event loop cannot learn that work arrived, so it must poll"
status: open
type: enhancement
area: [core, executor, rmw]
severity: medium
found: 2026-10-29
related: [2143, 2145, 2146]
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
