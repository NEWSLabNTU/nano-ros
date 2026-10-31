# Embassy Integration

**Embassy is not supported.** nano-ros supports a way of running only if it
can keep its real-time promise there: nano-ros decides which callback runs
next and at what priority, so it can state when a callback runs, how late it
can be, and that a low-priority callback never holds up a high-priority one
([RFC-0113](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0113-rt-unified-wait.md)).

Under Embassy the executor would run inside an Embassy task, and Embassy's
scheduler, configured by the application, would decide when that task and
every callback in it run. nano-ros cannot analyse that schedule, so it cannot
make the promise, and a supported shape whose timing nobody can state is a
promise nobody can check.

What this page used to describe is gone with it: the hand-driven spin task,
`Executor::spin_async` / `spin_async_with_sleep`, the readiness notifier and
the host-owned `nros::setup!` assembly (RFC-0113 D7, D10).

## What to use instead

- **An RTOS or bare-metal board through `nros::main!`.** Tiers become OS
  threads (or the bare-metal loop) at priorities nano-ros derives from your
  launch contracts.
- **RTIC** ([RTIC Integration](./rtic-integration.md)). RTIC's scheduling is
  analysable and `nros::main!` generates the task mapping, so the promise
  holds there.
