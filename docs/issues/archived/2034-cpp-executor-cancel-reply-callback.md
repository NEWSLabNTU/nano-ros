---
id: 2034
title: "The C++ callback tier's cancel replies are taken only when a future is polled, never by the executor's spin"
status: resolved
type: enhancement
area: [api]
severity: low
found: 2026-10-24
resolved: 2026-10-25
related: [2030, 2027, 2009, phase-515]
---

## What

Issue 2030 gave the raw action-client arena entry a cancel-response callback
slot. When that slot is set, the executor's spin takes each cancel reply, which
frees its request slot (`ActionRequests`, `MAX_CONCURRENT_GOALS` = 4). C sets
it. C++ registers `None` (`packages/api/nros-cpp/src/action.rs`), because its
readers take the reply themselves:

- `Client<A>::try_recv_cancel_response`;
- the `async_cancel_goal` / `async_cancel_all_goals` / `async_cancel_goals_before`
  futures (issue 2027);
- `poll()`.

If the spin took the reply first, those readers would find nothing.

So a C++ caller that cancels and never polls the future, never calls `poll()`
and never reads the reply still fills the four slots, and its fifth cancel is
refused. Upstream's `rclcpp` delivers the cancel response from the executor.

## Why it is not a one-liner

The spin would need to hand each reply to the reader that sent its request:

- a per-request stash, keyed by the request's sequence id (the cancel
  sub-client's correlator already holds the goal id per request);
- the futures and `try_recv_cancel_response` then collect from the stash;
- a reply nobody collects needs an eviction rule.

The arena entry's dispatch has the slot. The work is in the C++ side's storage
and in the bound.

## Where it is written

The porting guide (`book/src/getting-started/porting-a-cpp-node.md`) and the
C++ API guide (`docs/guides/cpp-api.md`) both say that cancel replies are taken
by the future or by `poll()`, not by the spin.

## Resolution

Resolved 2026-10-25. The C++ client now registers the arena entry's
cancel-response slot (issue 2030), so the executor's spin takes every cancel
reply, which frees its request's slot.

- The core callback (`RawCancelResponseCallback`) now also carries the request's
  sequence id and the whole `CancelGoal_Response` frame. The C trampoline
  ignores both.
- `nros-cpp` forwards each reply to a new per-client C++ callback,
  `nros_cpp_action_client_set_cancel_reply_callback`, which
  `rclcpp_action::Client<A>` installs at creation. `nros_cpp_action_cancel_reply_decode`
  reads the frame. `nros_cpp_action_client_poll` takes cancel replies the same
  way, so a future's own pump still works. A blocking wait forwards cancel
  replies to the client's callback.
- `Client<A>::on_cancel_reply` hands the reply to the `async_cancel_*` waiter
  whose request it answers (by sequence id, else by goal id). It settles the
  future and runs the callback inside the spin.
- A reply no waiter claims goes into a fixed ring in the client
  (`CANCEL_STASH_CAP` = 4, the number of cancels in flight per client). The
  oldest entry is evicted when the ring is full. `try_recv_cancel_response`
  reads the ring first, then the wire. The ring has no heap on any build and
  replaces the hosted-only `std::vector`.
- `CppActionClient` grew one function pointer. The layout mirror
  (`nros::sizes::CppActionClientLayout`), the hand-math fallback and the
  buildless snapshot (48 -> 56) all moved with it.

Held by `packages/api/nros-cpp/tests/compile/action_cancel_reply_spin_runtime.cpp`
in `just check cpp`. With only `rclcpp::spin_some`, six cancels are all sent,
the kept codes come back oldest first with the two oldest evicted, and an
`async_cancel_goal` callback and future complete inside the spin. Six
discarded `async_cancel_all_goals` futures also free their slots.
Negative control, measured: restoring `cancel_response_callback: None` in
`nros-cpp/src/action.rs` makes the test fail.

## Still open

Nothing for this issue. `try_recv_cancel_response` returns only the return
code. A caller that needs the `goals_canceling` list of an unclaimed reply
uses `async_cancel_*`.
