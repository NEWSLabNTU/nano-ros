---
id: 278
title: "No polling subscriber / blocking service futures — mrm_handler-class nodes must weaken to cache-latest + send-and-poll"
status: resolved
type: enhancement
area: nros-cpp
related: [0254, 0290]
resolved_in: phase-278-polling-primitives
---

## Resolution (2026-08-02)

Both idioms now have a nano-ros equivalent:

- **`nros::PollingSubscription<M>`** (`polling_subscription.hpp`,
  `Node::create_polling_subscription`) — latest-value polling subscriber
  (`take_data`/`take_new_data`/`take`/`peek`, drain-to-latest cache). Replaces
  the hand-rolled "callback caches latest + has_ flag" pattern. Zero ABI change.
- **`nros::Client<Svc>::call_polling(req, resp, timeout_ms)`** — a bounded
  service call that does NOT spin the executor (send + sleep-poll via
  `nros_platform_sleep_ms`, never `spin_once`), so it is safe from inside a
  subscription/timer callback where `call()`/`Future::wait` return `Reentrant`
  (#0290). Built on the existing `nros_cpp_service_client_call_raw` (which
  already had the send + sleep-poll shape); this added a caller `timeout_ms`
  parameter and the C++ method. **Callback-safe only on multi-threaded backends**
  (zenoh MT, cyclonedds), where the backend read task delivers the reply into
  the client's queue while the loop yields; on single-threaded/polled backends
  the reply needs a `spin_once` the callback blocks, so it times out there — use
  `call()` from the main loop. Keep the timeout short (it blocks the dispatch
  thread).

Both gated by `-fsyntax-only` instantiation compile tests
(`tests/compile/{polling_subscription,service_client_call_polling}.cpp`) in
`just check cpp`. See the mrm_handler usage in the standalone-vs-workspace
examples. Not done here (follow-ups): a native-Rust parallel of
`PollingSubscription`, and porting the real Autoware `mrm_handler`.
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

## Progress

**Half A LANDED (2026-08-02) — `nros::PollingSubscription<M>`.** A C++ wrapper
(`polling_subscription.hpp`, `Node::create_polling_subscription`) that owns a
poll-mode `Subscription<M>` plus a retained latest value, read repeatably via
`take_data()` (cached-or-new latest), `take_new_data()` (only if new this call),
`take(M&)`, `peek()`. Each accessor drains to the newest pending sample (a burst
collapses to its last element, matching `InterProcessPollingSubscriber::takeData`)
then answers from the cache. Pure wrapper over the existing consuming `try_recv`
— zero C/Rust/executor ABI change; directly replaces the hand-rolled
`topic_state_monitor.cpp` cache pattern. Instantiation gated by a `-fsyntax-only`
compile test (`tests/compile/polling_subscription.cpp`) in `just check cpp`.

**Half B (bounded service call inside a callback) — shared-session question
DE-RISKED (2026-08-02).** #0290 fixed the safety hole (in-callback bounded wait
now returns `Reentrant` instead of aliasing `&mut Executor`). The tractable path
is an L1 (executor-free) service client with a bounded `call(req, resp, timeout)`.

The reentrancy worry was: the L1 client **shares the node's session**
(`nros_client_init_polling` → `resolve_session_and_domain(node)`), so wouldn't a
callback-side call re-enter the session the executor is mid-spin on? Answer: no,
because `RawServiceClient` is pure **send + poll-a-queue** (`send_request_raw`,
`try_recv_reply_raw`) — it never *drives* the session's I/O. The reentrancy
resolves entirely on HOW the reply reaches the queue, which is platform-split:

- **Multi-threaded backends (zenoh MT, cyclone):** a background read task
  delivers the reply into the client's queue asynchronously — the zpico read
  task runs `pending_get_reply_handler` → fills the pending-get slot + fires
  `reply_waker`, independent of the executor's `spin_once` (verified against the
  #348/#376 reply path). A callback-side `call(timeout)` is `send_request_raw` +
  a wall-clock loop of `try_recv_reply_raw` + a short sleep — it touches ONLY the
  client's own reply queue, never re-drives the shared session, so it is **safe
  from inside a callback**. The naive `Promise::wait` failed precisely because
  it called `spin_once` (reentrancy); the L1 path must NOT — it sleeps + polls
  and lets the read task deliver.
- **Single-threaded / polled backends (bare-metal smoltcp/serial, zenoh-pico
  single-thread):** rx is spin-driven — the reply can only land when `spin_once`
  drives the session, which the callback is *blocking*. So a blocking call from a
  callback is **fundamentally impossible** there; send-and-poll (the current
  mrm_handler weakening) is the correct design.

**Design for Half B:** add a bounded `call_polling(req, resp, timeout)` to the L1
`RawServiceClient` (send + sleep-poll `try_recv_reply_raw`, never `spin_once`) +
the C/C++ wrappers; document it as multi-threaded-backend-only and keep
send-and-poll as the single-threaded fallback. No reentrant executor needed.

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
