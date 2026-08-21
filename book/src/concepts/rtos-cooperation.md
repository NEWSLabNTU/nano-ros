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

The knobs live on **`SpinOptions`** (passed to
`Executor::spin_blocking(opts)`), not on `ExecutorConfig` — the config
struct carries identity/transport (locator, domain, node name, clock),
while scheduling shape is a property of each spin call:

| Knob | Default | When to change |
|------|---------|----------------|
| `SpinOptions::max_callbacks(n)` | `None` (drain all ready work) | Set to `1` for upstream-`rclcpp`-style "one callback per iteration" — gives the executor a chance to re-check timers / GCs / yield between callbacks |
| `SpinOptions::timeout_ms(ms)` | `None` (spin forever) | Bound one spin call by wall clock — the time-triggered pattern calls `spin_blocking` once per cycle with the cycle's ROS slot as the timeout |
| `SpinOptions::spin_once()` / `only_next` | off | One round of work then return — the cooperative-loop building block |

Backends opt into one additional behaviour automatically:
`Session::next_deadline_ms()` tells the executor about the backend's
next internal event (lease keepalive, heartbeat). The executor caps
`drive_io`'s timeout against it. No app configuration; transparent
optimization.

## Per-model recommended configuration

### Cooperative single-task

```rust
// One dedicated task; drain everything each round.
executor.spin_blocking(SpinOptions::default())?;
```

Drain everything; one task, no fairness concern.

### Preemptive priority RTOS — recommended

```rust
loop {
    executor.spin_blocking(SpinOptions::new().max_callbacks(1))?;
}
```

`max_callbacks(1)` matches upstream's `rclcpp`
single-threaded executor pattern. Each `spin_once` fires one
callback and then re-checks timers + GCs. ROS-internal entities
share the task priority, but the spin-loop iteration is the
scheduling unit; timer expiries are bounded by *one* callback's
WCET, not the sum across all ready callbacks.

If max-callback dispatch latency is still too high in this profile
(e.g., a single callback is slow), the remaining bound is that one
callback's WCET — split the slow callback, or move it to its own
tier/task (see [Scheduling Models](../internals/scheduling-models.md)).

### WCET-bounded real-time (RTIC / Embassy)

Don't use the spin loop. Use the async path — a buffered subscription
handle whose `recv()` is an async fn with waker integration:

```rust
let mut sub = node.create_subscription::<MyMsg>("/topic")?;
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
// Called once per cycle from the cyclic frame:
executor.spin_blocking(SpinOptions::new().timeout_ms(5))?;   // 5 ms ROS slot
```

The cycle gives ROS a fixed wall-clock slot; the `timeout_ms` bound
returns control when the slot expires regardless of pending work.
Pending work resumes next cycle. (There is no finer-grained
per-callback budget knob today — if a single callback can overrun the
slot, that callback's WCET is your design constraint.)

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
| drain-all (default) | High | Bounded by ALL ready callbacks' total WCET | Poor under load | Smallest |
| `max_callbacks(1)` | Slightly lower (more spin loop iterations) | Bounded by ONE callback's WCET | Good | Same — the cap is just an integer |
| `timeout_ms(N)` per cycle | Lower | Bounded by N ms wall clock + one callback's WCET | Good if N tight | One clock read per iteration |
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
