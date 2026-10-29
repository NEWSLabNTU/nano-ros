---
id: 2145
title: "`spin_async()` is a busy loop that blocks its runtime thread up to 1 ms per poll, and the book says it is waker-driven"
status: open
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
