---
id: 1195
title: "`Promise::wait` polls on a fixed 10 ms grid and overshoots timeouts shorter than that"
status: resolved
resolved: 2026-10-15
type: enhancement
area: executor
related: [issue-1192, phase-436]
---

## Problem

`Promise::wait` ignores the wake infrastructure and polls:

```rust
let spin_interval = core::time::Duration::from_millis(DEFAULT_SPIN_INTERVAL_MS);
let timeout_ms = timeout.as_millis().min(u64::MAX as u128) as u64;
let max_spins = (timeout_ms / DEFAULT_SPIN_INTERVAL_MS).max(1);
```
(`packages/core/nros-node/src/executor/handles.rs:2328`, with
`DEFAULT_SPIN_INTERVAL_MS = 10` at `handles.rs:13`)

Two consequences:

1. **Reply latency is quantized to 10 ms** regardless of when the reply lands,
   on backends that *do* have an async wake source. Cyclone installs a real
   `dds_set_listener`/`on_data_available` (`vtable.cpp:322`,
   `session.cpp:105`), so the wake exists and this path does not use it.

2. **A timeout shorter than 10 ms overshoots.** `max_spins` floors at 1, so
   `wait(Duration::from_millis(1))` still runs one full `spin_once(10ms)` —
   the caller's 1 ms deadline is exceeded by up to 10x before `WaitBudget`
   is consulted.

This does not violate the "never blocks forever" property — the loop is
bounded — but it is the one place in the executor where the waker
infrastructure exists and is bypassed.

## Fix direction

Spin with `min(remaining_budget, DEFAULT_SPIN_INTERVAL_MS)` so the last
iteration cannot overshoot the caller's deadline, and let the underlying
`spin_once` block on the wake primitive rather than returning on a fixed grid.
The existing `WaitBudget` already tracks the deadline; it is consulted one
spin too late.

## Resolution (2026-10-15, phase-436 D1)

Both consequences are gone. The first was half right: Cyclone, the backend this
issue named, was never quantized — and zenoh, the default backend, was, for a
reason this issue did not name.

**Consequence 2** was fixed by phase-436 W4: `WaitBudget::next_spin_interval`
never hands `spin_once` more than the remaining budget, so `wait(1ms)` waits
about 1 ms.

**Consequence 1 — measured.** The 10 ms constant is the CEILING of each
`spin_once`, not its quantum, but only when the reply's arrival reaches the
executor's wake. With an async wake installed, `spin_once` parks on the
platform wake object (`nros_platform_wake_wait_ms`) and returns as soon as the
backend fires the runtime wake callback.

Harness, kept out of the tree: copies of `examples/linux/rust/service-
{server,client}` built with `nros build <image>`, the client's tick replaced by
300 back-to-back `call_for_name` calls timed with `std::time::Instant`, the
first 20 discarded as warm-up. Both processes native on the host (aarch64
Linux), private `ROS_DOMAIN_ID`. `call_for_name` reaches
`RuntimeClientDispatch::call_raw`, which loops `spin_once(10 ms)` exactly as
`Promise::wait` does. Per-call wall time, n = 280 each:

| backend | build | p50 | p90 | p99 | calls >= 9 ms |
|---|---|---|---|---|---|
| Cyclone | main | 77 us | 112 us | 157 us | 0 |
| Cyclone | main, rerun | 66 us | 96 us | 148 us | 0 |
| zenoh (`rmw_zenohd` on loopback) | main | 10108 us | 10130 us | 10320 us | 280 |
| zenoh | fixed | 189 us | 233 us | 1180 us | 0 |

- **Cyclone**: the participant-level `on_data_available` listener
  (`nros-rmw-cyclonedds/src/session.cpp`) calls the runtime wake callback when
  the reply reader has data, so the park ends early. Nothing to fix. A busy-spin
  would also return fast, so that was checked: an idle client (no server,
  spinning forever) used 0.2 % of a CPU over 5 s, so the slice really parks.
- **zenoh**: every round trip cost one whole slice. Replies and requests are
  delivered on zenoh-pico's read task, where `drive_io`'s poll path never sees
  them, and of the shim's three arrival callbacks only the subscriber's fired
  the runtime wake (issue 0317 added it there). `reply_waker_callback` woke
  only the async `REPLY_WAKERS`, and `queryable_callback` only the async waker
  and the legacy std cvar, so the client's `Promise::wait` and the server's
  spin each slept out their slice. Both now call `fire_runtime_wake()`
  (`nros-rmw-zenoh/src/shim/service.rs`). Regression test:
  `shim::service::tests::reply_arrival_fires_the_runtime_wake`, which fails
  with the reply-side call removed. The executor-level half — a wake signalled
  while `spin_once` is parked cuts the park short, with a lower bound so a spin
  that never waited cannot pass — is already pinned by
  `packages/api/nros-c/tests/run/executor_backend_wake.c` (issue 1385).

A permanent latency assertion against a live backend pair was not added: it
would test the host scheduler as much as the code. The fix is pinned at the
callback instead, where the defect was.
