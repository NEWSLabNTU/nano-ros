---
id: 2145
title: "`spin_async()` is a busy loop that blocks its runtime thread up to 1 ms per poll, and the book says it is waker-driven"
status: resolved
resolved: 2026-10-31
type: bug
area: [core, executor, docs]
severity: medium
found: 2026-10-29
related: [2144, 2134]
---

## What

`Executor::spin_async` (`packages/core/nros-node/src/executor/spin.rs:13516`)
is:

```rust
loop {
    self.spin_once(core::time::Duration::from_millis(1));
    core::future::poll_fn::<(), _>(|cx| { cx.waker().wake_by_ref(); Poll::Pending }).await;
}
```

So:

1. **It busy-loops.** It wakes itself before returning `Pending`, so every
   runtime that honours wakes polls it again immediately. One core runs at
   100 % while the app is idle.
2. **It blocks the runtime's thread.** `spin_once(1 ms)` parks on the platform
   primitive (RFC-0106), inside `poll`. On a single-threaded runtime (tokio
   `current_thread`, the shape `examples/linux/rust/service-client-async` and
   `action-client-async` use, or Embassy's single executor), every other future
   waits up to 1 ms per cycle.

The rustdoc says the first half honestly ("CPU-bound by construction ... Do
not read `async` as idle"). The book does not.
`book/src/concepts/rtos-cooperation.md:123` says "The executor's `spin_async`
drives futures via wakers". That is false, and it is the page a reader picks a
spin mode from.

## Direction

- **Now:** correct the book line to match the rustdoc, with no code change.
- **Fix:** once issue 2144's notifier exists, rewrite `spin_async` as
  `spin_some(ZERO)`, then register the task's `Waker` with the notifier and an
  executor timer at `next_deadline_us()` (issue 2143), then return `Pending`.
  It must never block inside `poll`. Acceptance is a test that measures idle
  CPU, plus a test showing a sibling future's latency is not bounded below by
  1 ms.

This also decides Embassy's Tier 0 recipe (issue 2134): while `spin_async`
looks like this, an Embassy task driving nano-ros is a busy loop.

## Progress

**2026-10-30 — the blocking half is fixed and the book is corrected; the
busy loop stays until issues 2143/2144 land.**

- **Never blocks inside `poll`.** `spin_async` now runs one
  `spin_once(Duration::ZERO)` pass per poll — the non-blocking path, which
  parks on nothing — instead of `spin_once(1 ms)`. A sibling future on a
  single-threaded runtime waits at most one pass's callbacks, never a park.
  One pass rather than `spin_some(ZERO)`'s drain, deliberately: `spin_some`
  with `ZERO` has no bound, so a callback that keeps feeding itself work would
  hold the runtime thread inside a single poll. A pass then a yield gives the
  runtime a turn between passes. When the notifier lands, the same shape works:
  re-wake if the pass did work, otherwise register and return `Pending`.
- **Test:** `executor::tests::spin_async_poll_never_parks_and_wakes_itself`
  (`packages/core/nros-node/src/executor/tests.rs`) polls the future three times
  with a counting waker and asserts the executor's own record of the park
  (`last_park()` / `last_park_achieved_us()`) is 0, plus one self-wake per poll.
  It reads the executor's park record instead of a wall clock, so it cannot
  flake under load. Mutation-checked: putting `from_millis(1)` back fails it
  with `left: 1000, right: 0`.
- **Docs:** the rustdoc envelope, `book/src/concepts/rtos-cooperation.md`
  (the "drives futures via wakers" line, the opening paragraph and the
  trade-off row), `embassy-integration.md`, `ros2-comparison.md`,
  `rmw-vs-upstream.md`, `no-std.md`, `docs/reference/std-alloc-requirements.md`,
  RFC-0001 (table row, plus an "as built" note saying the async sequence
  diagram is the target shape) and RFC-0112's Readiness-out row, and the
  `rust:Executor::spin_async` parity-ledger row.

**Trade-off to know about.** The old 1 ms park was also the loop's only
sleep, so an idle `spin_async` used to sit parked most of each cycle. With
the park gone there is no wait at all: idle CPU is now a full 100 % of one
core on every backend, where before it was 100 % only in effect. That is the
price of not blocking the runtime's thread, and it is what the remaining work
removes.

**Left (unblocked: issues 2143 and 2144 have both landed):**

- Rewrite the yield so the task sleeps until work arrives: register the
  task's `Waker` with the executor-level readiness notifier
  (`set_ready_notifier`, issue 2144) and with a timer at
  `next_deadline_us()` (issue 2143), and return `Pending` without a
  self-wake when the pass did no work. Two design questions are open. The
  notifier exists only with `alloc` + `rmw-cffi` and a backend that answers
  `ready_notifier_supported()`, so a poll-only backend (XRCE, smoltcp) still
  needs the self-wake. And a runtime-agnostic future owns no timer, so the
  deadline half needs either a runtime hook or the caller's own timer.
- Acceptance tests from **Direction**: one measuring idle CPU (needs that
  notifier), and one showing a sibling future's latency is not bounded below by
  1 ms (the park test above covers the mechanism; an end-to-end latency
  measurement is still open).
- Issue 2134's Embassy Tier 0 recipe keeps its own polling loop until then.

## Resolution

**2026-10-31 — `spin_async` sleeps.** Both halves the Direction asked for, on
the two primitives issues 2143 and 2144 landed.

- **Arrivals.** The executor's wake context (`WakeCtx`) holds a second waker
  slot beside the host notifier: `task_waker`, an `atomic_waker::AtomicWaker`.
  `WakeCtx::notify` wakes it on every arrival it already tells the host
  about, so `spin_async` and a host's own `set_ready_notifier` coexist rather
  than displacing each other. Each poll registers the task's `Waker` there
  BEFORE its pass, so an arrival during or after the pass re-polls the task
  and nothing is lost. A pass that did no work then returns `Pending` with no
  self-wake.
- **Deadlines.** A runtime-agnostic future owns no timer, so this is a
  sibling, `Executor::spin_async_with_sleep(sleep)`. It takes the runtime's
  sleep (`tokio::time::sleep`, Embassy's `Timer::after_micros`) and waits for
  whichever comes first, an arrival or `sleep(next_deadline_us())`. Plain
  `spin_async()` is that loop without the sleep, so with any deadline pending
  it still yields with a self-wake. On zenoh that is always, because its
  3.3 s keepalive is a deadline. The book therefore recommends
  `spin_async_with_sleep`, and the two tokio examples
  (`service-client-async`, `action-client-async`) use it.
- **Poll-only backends** (`ready_notifier_supported()` false: XRCE, uORB's
  polling build, bare-metal smoltcp) still self-wake every poll. They learn of
  an arrival only inside a pass, so there is nothing to sleep on. Documented
  in the rustdoc and the book.

**Acceptance, live over zenoh** (`packages/testing/nros-tests/tests/entity_wake.rs`):

- `spin_async_sleeps_while_idle_and_wakes_on_an_arrival` — 0 wakes in 1 s
  idle, then a publish is dispatched inside a park that only a wake can end.
  Mutation-checked: forcing the old self-wake gives 19,995 wakes in the same
  second.
- `a_sibling_future_never_waits_a_spin_async_park` — a `spin_async` poll,
  which is what a sibling on a single-threaded runtime waits for, takes about
  60 µs median against the old 1 ms floor.

The unit test `spin_async_poll_never_parks_and_wakes_itself` stays: the mock
session is a poll-only executor, so it now pins that fallback.

Docs updated: the rustdoc envelope, `rtos-cooperation.md` (Async runtime
section and trade-off row), `embassy-integration.md`, `ros2-comparison.md`,
`rmw-vs-upstream.md`, `no-std.md`, `std-alloc-requirements.md`, RFC-0001's
"as built" note, RFC-0112's Readiness-out row and D2 list, the
`rust:Executor::spin_async` parity-ledger row, and the `nros` services guide.

Issue 2134's Embassy recipe can now use `spin_async_with_sleep` instead of
its own polling loop. The book's Embassy chapter says so; rewriting the
chapter around `setup!` remains phase-520 W4.
