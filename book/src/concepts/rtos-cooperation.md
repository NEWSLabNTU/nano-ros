# RTOS Cooperation

nano-ros runs on platforms that span pure cooperative bare-metal
through multi-task preemptive RTOS to fully async / Future-driven
runtimes. The executor's spin loop has to cooperate with each of
these without imposing a single execution model on all of them.

This page maps the common RTOS / runtime execution profiles to the
configuration knobs the executor exposes. New apps pick a profile
that matches their target's scheduling discipline; the knobs
translate that choice into bounded behaviour from `drive_io`.

## The execution model spectrum

| Model | Description | Example targets |
|-------|-------------|-----------------|
| **Cooperative single-task** | One task / thread does all ROS work. No preemption from other tasks (there are none, or they're all lower priority). Yielding happens at task boundaries. | Bare-metal MPS2-AN385, single FreeRTOS task, single Zephyr thread |
| **Preemptive priority** | ROS task runs at a fixed priority. Higher-priority tasks preempt mid-call by the kernel. ROS-internal entities (subs, timers, services, GCs) all share that priority — they don't preempt each other. | Typical FreeRTOS / ThreadX / Zephyr deployment with worker tasks |
| **WCET-bounded real-time** | Each "task" has a provable worst-case execution time. Tasks are dispatched directly from interrupts; no spin loops in the hot path. | RTIC, Embassy, avionics WCET-validated code |
| **Time-triggered cyclic** | Fixed schedule. Each cycle does a fixed amount of work in a fixed time slot; ROS gets a fraction of the cycle and must yield. | DO-178C / IEC 61508 controller frames |
| **Async runtime** | Futures registered with wakers; reactor drives. No spin loop visible to user code. | tokio, Embassy futures, custom async runtimes |

## How `drive_io` behaves by default

The executor's `spin_once(timeout_ms)` calls
`session.drive_io(...)` and lets it drain all ready I/O before
returning. After `drive_io` returns, the executor processes any
expired timers and triggered guard conditions. If 10 messages
arrived during the wait, all 10 callbacks fire in a single
`spin_once`, then timer / GC dispatch happens once afterwards.

This is the right default for **cooperative single-task** apps and
for **async-runtime** apps using `spin_async`. Both want
throughput; neither benefits from per-callback scheduling
opportunities.

For the other three models, the default has trade-offs the
configuration knobs address.

## Configuration knobs

| Knob | Default | When to change |
|------|---------|----------------|
| `ExecutorConfig::max_callbacks_per_spin` | `usize::MAX` (drain all) | Set to `1` for upstream-`rclcpp`-style "one callback per `spin_once`" — gives the executor a chance to re-check timers / GCs / yield between callbacks |
| `ExecutorConfig::time_budget_per_spin_ms` | `None` (no budget) | Set to fixed wall-clock budget for time-triggered apps — `drive_io` returns when the budget expires regardless of pending work |
| `ExecutorConfig::spin_period_ms` | platform-dependent | Tighten for lower worst-case latency; loosen for less CPU spent in the spin loop |

Backends opt into one additional behaviour automatically:
`Session::next_deadline_ms()` tells the executor about the backend's
next internal event (lease keepalive, heartbeat). The executor caps
`drive_io`'s timeout against it. No app configuration; transparent
optimization.

## Per-model recommended configuration

### Cooperative single-task

```rust
ExecutorConfig {
    max_callbacks_per_spin: usize::MAX,    // default — drain all
    time_budget_per_spin_ms: None,         // default — no budget
    spin_period_ms: 1,                     // tight loop on the dedicated task
    ..Default::default()
}
```

Drain everything; one task, no fairness concern. Spin tightly to
keep latency low.

### Preemptive priority RTOS — recommended

```rust
ExecutorConfig {
    max_callbacks_per_spin: 1,             // one callback per spin_once
    time_budget_per_spin_ms: None,
    spin_period_ms: 1,
    ..Default::default()
}
```

`max_callbacks_per_spin = 1` matches upstream's `rclcpp`
single-threaded executor pattern. Each `spin_once` fires one
callback and then re-checks timers + GCs. ROS-internal entities
share the task priority, but the spin-loop iteration is the
scheduling unit; timer expiries are bounded by *one* callback's
WCET, not the sum across all ready callbacks.

If max-callback dispatch latency is still too high in this profile
(e.g., a single callback is slow), the next refinement is moving
timer and guard-condition dispatch *into* `drive_io`'s loop so the
`max_callbacks = 1` cap covers them too. This is the path where one
slow sub callback no longer delays a timer that should have fired
mid-callback. (Phase 106 work.)

### WCET-bounded real-time (RTIC / Embassy)

Don't use the spin loop. Use the async path:

```rust
let executor = Executor::open_async(&config)?;
let sub = node.create_subscription_async::<MyMsg>("/topic")?;
loop {
    let msg = sub.recv().await?;        // suspends; waker integration
    handle(msg);
}
```

The async path doesn't go through `drive_io` at all. Subscriptions
register a `Waker`; the backend's RX path wakes the waker; the
async runtime schedules the receiving task. Per-task WCET analysis
applies to each `recv().await` continuation, not to a spin loop.

### Time-triggered cyclic

```rust
ExecutorConfig {
    max_callbacks_per_spin: usize::MAX,    // not the bottleneck here
    time_budget_per_spin_ms: Some(5),      // 5 ms ROS budget per cycle
    spin_period_ms: 5,                     // matches the cycle's ROS slot
    ..Default::default()
}
```

The cycle gives ROS a fixed wall-clock slot. `time_budget_per_spin_ms`
bounds time spent in `drive_io` regardless of pending work. The
backend respects the budget by checking elapsed wall-clock between
callbacks and returning when exceeded. Pending work resumes next
cycle.

### Async runtime

`drive_io` not used in the hot path. The executor's `spin_async`
drives futures via wakers; `drive_io` becomes a polling tick
internally with negligible overhead.

```rust
executor.spin_async().await
```

No knobs apply.

## Trade-offs at a glance

| Configuration | Throughput | Per-callback latency | Timer-callback fairness | Code-size cost |
|---------------|-----------|---------------------|-------------------------|----------------|
| `max_callbacks = MAX` (default) | High | Bounded by ALL ready callbacks' total WCET | Poor under load | Smallest |
| `max_callbacks = 1` | Slightly lower (more spin loop iterations) | Bounded by ONE callback's WCET | Good | Same — the cap is just an integer |
| `time_budget = Some(N)` | Lower (clock reads per callback) | Bounded by N ms wall clock | Good if N tight, fair if N loose | One clock read per callback (~10–50 ns) |
| async / `spin_async` | Per-future | Per-future Future poll | Cooperative — futures yield voluntarily | Async runtime cost |

## Backends and their wait primitives

`drive_io`'s sleep behaviour is backend- and platform-dependent. The
spin loop's "where does the thread sleep" question maps as:

| Platform | Sleep primitive in drive_io | When CPU is sleeping |
|----------|----------------------------|----------------------|
| POSIX | `select` / `epoll_wait` with deadline | Inside drive_io |
| Zephyr | `k_poll` / condvar with deadline | Inside drive_io |
| FreeRTOS | `xSemaphoreTake(g_spin_sem, ticks)` | Inside drive_io |
| NuttX | `sem_timedwait` with absolute deadline | Inside drive_io |
| ThreadX | `tx_event_flags_get(..., TX_OR, ..., ticks)` | Inside drive_io |
| Bare-metal smoltcp + `BoardIdle` | smoltcp poll + `wfi()` between iterations | Outside drive_io (in the spin loop's idle hook) |
| Bare-metal smoltcp without `BoardIdle` | smoltcp poll, busy loop | Nowhere — CPU spins |

In all cases the user-visible API is `Executor::spin_once(timeout)`;
the platform-correct sleep happens transparently underneath.

## See also

- [RMW API Design](../design/rmw.md) — the architectural reasons
  the runtime / RMW boundary is shaped the way it is.
- [RMW API: Differences from upstream `rmw.h`](../design/rmw-vs-upstream.md)
  Section 4 — the `drive_io` vs `rmw_wait` comparison this page
  expands on.
- [no_std Support](no-std.md) — heap and threading constraints that
  shape the cooperative model.
