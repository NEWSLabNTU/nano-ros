---
id: 2034
title: "The C++ callback tier's cancel replies are taken only when a future is polled, never by the executor's spin"
status: open
type: enhancement
area: [api]
severity: low
found: 2026-10-24
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
