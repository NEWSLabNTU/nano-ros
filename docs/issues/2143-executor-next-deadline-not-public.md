---
id: 2143
title: "A host that drives the executor cannot ask when it next needs to run: `next_wake_bound_us` is `pub(crate)`"
status: open
type: enhancement
area: [core, executor]
severity: medium
found: 2026-10-29
related: [2144, 2145]
---

## What

RFC-0112 D2's Tier 0 contract needs a **Time out** part: a host loop (tokio,
GLib, Arduino `loop()`, a PX4 work item, a plain C `while`) that calls
`spin_some` must know how long it may sleep before the executor next needs to
run.

The executor already computes exactly that. `next_wake_bound_us` (RFC-0106) is
the minimum over the caller's budget, the next timer, the backend's next event,
a supervised route's deadline and every registered wake source. But it is
`pub(crate)`
(`packages/core/nros-node/src/executor/spin.rs:4395`, and
`next_wake_bound_attributed_us` at `:4486`).

What is public covers only parts of that minimum:

- `route_next_due_us()` (`:4332`) gives supervised routes only.
- `timer_time_until_next_call_ns(id)` (`:11798`) gives one timer per call, so
  the host must enumerate the timers itself and still misses the backend and
  the wake sources.

So today a host either polls on a fixed period, which oversleeps a timer or
wakes the CPU for nothing, or calls `spin_once(t)` with a guess and gives the
executor the thread.

## Direction (RFC-0112 D2.2)

Add `Executor::next_deadline_us() -> Option<u64>`, a public wrapper over
`next_wake_bound_us` with no caller budget. `None` means nothing is scheduled,
so the host waits on readiness alone (issue 2144). Export it through the C and
C++ APIs in the same change. Add a test that registers a 10 ms timer and checks
for a value of at most 10 ms, plus a lower-bound check, as CLAUDE.md requires
for timing.
