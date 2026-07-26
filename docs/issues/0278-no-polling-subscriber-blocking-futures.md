---
id: 278
title: "No polling subscriber / blocking service futures — mrm_handler-class nodes must weaken to cache-latest + send-and-poll"
status: open
type: enhancement
area: nros-cpp
related: [0254, 0290]
---

## Finding (autoware-safety-island-example ports, 2026-07-24 — porting-notes 14)

Two upstream idioms have no nano-ros equivalent:

- `autoware_utils::InterProcessPollingSubscriber` (pull-based take) — ported
  as callback subs caching latest + `has_` flag. Adequate, but a take-style
  API would keep ports verbatim.
- Blocking service futures inside callbacks: `requestMrmBehavior` does a
  10 ms `future.wait_for` inside a timer callback. A blocking wait would
  need nested executor spin; the port weakened to send-and-poll (replies
  drained on later ticks, "success" = request sent). Behavior delta
  documented in-source, but it IS a semantic weakening of the safety path.

## Direction

- take()-style subscription accessor.
- Bounded-wait service call usable inside executor context, or an async
  callback-continuation form the compat layer can express.

## Correction + current state (2026-07-26)

Re-checked both halves against the code. Two things in the original write-up
were wrong or have since changed.

### The "RMW already caches latest per sub" premise is FALSE

The Direction above originally claimed a take() accessor would just expose an
existing cache. There is no such cache. `grep` for
`last_sample|latest_sample|cached_sample` across `packages/` finds nothing, and
none of the layers retain a readable last value:

- `nros-rmw/src/traits.rs` — `try_recv_raw` and friends all return
  `Result<Option<..>>`, draining.
- `nros-node/src/executor/handles.rs` — same shape, consuming.
- `nros-node/src/executor/triple_buffer.rs` is KEEP_LAST(1), but its reader is
  consuming-on-new-data (`:125-128`) and it is internal to the callback
  dispatch arena.

So this half needs NEW retained storage plus a decision about who owns it —
materially more work than the issue implied.

What DOES exist is a consuming non-blocking pull family, which covers the
"pull-based" part but not the "latest value, repeatably" part:
`Subscription::{try_recv, try_recv_raw, try_recv_validated, try_borrow,
try_recv_sequence}` (`nros-cpp/include/nros/subscription.hpp`), the blocking
`Stream::wait_next`, and on the C side `nros_subscription_init_polling` +
`nros_subscription_try_recv_*`. Each returns `TryAgain` when no NEW sample has
arrived, so emulating `InterProcessPollingSubscriber` still requires the
user-side "callback caches latest + `has_` flag" pattern.

### The bounded-wait API already exists — the gap was safety, not surface

`Client::call(req, resp, timeout_ms)` (`client.hpp`) and
`Future::wait(executor, timeout_ms, out)` (`future.hpp`) are exactly the
bounded wait this issue asks for, and work from the main loop.

The real problem was that using them from inside a callback was unsound, not
merely unsupported: C++ had no reentrancy guard while C did. Split out and
FIXED as **[0290](0290-cpp-missing-reentrancy-guard.md)** — the in-callback
case now returns `ErrorCode::Reentrant` instead of aliasing `&mut Executor`.

That closes the safety hole but not this issue: a clean error is not the same
as the call working from a callback. What remains here is the design question
of a nested-executor or callback-continuation form.
