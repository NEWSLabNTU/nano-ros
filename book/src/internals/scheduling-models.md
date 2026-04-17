# Scheduling Models

This chapter introduces the real-time scheduling models used by the platforms
nano-ros supports. Understanding these models helps you make informed
decisions about task priorities, deadline guarantees, and platform selection
for your application.

## Background: What Is Real-Time Scheduling?

A **real-time scheduler** decides which task (or thread) runs on the CPU at
any given moment. The key property is **predictability** — not raw speed. A
system that always finishes a 10 ms task in exactly 10 ms is more
"real-time" than one that finishes it in 1 ms but occasionally takes 50 ms.

Two key concepts recur across all models:

- **Priority**: a numeric value that determines which task runs when multiple
  tasks are ready. Higher-priority tasks preempt lower-priority ones.
- **Preemption**: the ability to interrupt a running task to run a
  higher-priority one. Non-preemptive (cooperative) systems only switch tasks
  at explicit yield points.

### Hard vs. Soft Real-Time

| Type | Guarantee | Consequence of missed deadline |
|------|-----------|-------------------------------|
| **Hard** | Deadline must never be missed | System failure (safety hazard) |
| **Soft** | Deadline should rarely be missed | Degraded quality (dropped frame, late message) |

nano-ros targets **soft real-time** by default (bounded message latency,
bounded memory). Hard real-time is achievable on RTIC and with careful
priority assignment on RTOS platforms.

## Scheduling Algorithms

### Fixed-Priority Preemptive (FPP)

The most common RTOS scheduling algorithm. Each task has a static priority
assigned at creation time. The scheduler always runs the highest-priority
ready task. When a higher-priority task becomes ready, it immediately
preempts the running task.

```
Time ──────────────────────────────────────►
        ┌───────┐           ┌───────┐
High    │ Task A│           │ Task A│        (preempts B when ready)
        └───┬───┘           └───┬───┘
            │ ┌───┐    ┌───┐   │ ┌───┐
Low         └►│ B │    │ B │   └►│ B │       (runs when A is blocked)
              └───┘    └───┘     └───┘
```

**Schedulability analysis** uses **Rate-Monotonic Analysis (RMA)**: assign
higher priority to tasks with shorter periods. RMA is optimal for
independent periodic tasks — if any fixed-priority assignment can meet all
deadlines, RMA can too.

The **response time** of a task under FPP is:

```
R_i = C_i + Σ ⌈R_i / T_j⌉ × C_j    (for all higher-priority tasks j)
```

Where `C_i` is worst-case execution time, `T_j` is the period of
higher-priority task `j`. The task meets its deadline if `R_i ≤ D_i`.

**Used by**: FreeRTOS, ThreadX, NuttX (SCHED_FIFO), Zephyr (preemptive
threads), RTIC.

### Round-Robin (Time-Sliced)

Tasks at the **same** priority level share CPU time in equal time slices
(quanta). When a task's quantum expires, the scheduler switches to the next
same-priority task. Tasks at different priority levels still follow FPP rules.

```
Time ──────────────────────────────────────►
        ┌──┐┌──┐┌──┐┌──┐┌──┐┌──┐
Pri 3   │A ││B ││A ││B ││A ││B │            (equal time slices)
        └──┘└──┘└──┘└──┘└──┘└──┘
```

Round-robin prevents starvation among equal-priority tasks but adds
scheduling jitter (a task may wait up to `(N-1) × quantum` before running,
where N is the number of same-priority tasks).

**Used by**: NuttX (SCHED_RR), FreeRTOS (when `configUSE_TIME_SLICING=1`),
ThreadX (when `time_slice > 0`).

### Cooperative (Non-Preemptive)

Tasks run until they explicitly yield the CPU. No preemption occurs. This
eliminates the need for locks (no race conditions) but requires every task
to yield frequently. A single task that runs too long blocks all others.

```
Time ──────────────────────────────────────►
        ┌──────────┐     ┌────┐
Task A  │ runs     │     │    │              (runs until yield())
        └────┬─────┘     └──┬─┘
             │ ┌────────┐   │ ┌──────┐
Task B       └►│ runs   │   └►│ runs │       (gets CPU only when A yields)
               └────────┘     └──────┘
```

**Used by**: Zephyr (cooperative threads via `K_PRIO_COOP`), bare-metal
super-loops, Embassy async executor.

### Interrupt-Driven (Hardware Scheduling)

The CPU's interrupt controller acts as a hardware scheduler. Each interrupt
source has a hardware priority level. When an interrupt fires, the hardware
saves context and jumps to the handler — no software scheduler overhead.
Nested interrupts provide preemption between priority levels.

On ARM Cortex-M, the **Nested Vectored Interrupt Controller (NVIC)** provides:
- Up to 256 priority levels (typically 8–16 usable)
- Zero-cycle context switch for tail-chaining interrupts
- Deterministic latency (12 cycles to handler entry on Cortex-M3)

```
                          NVIC
IRQ Priority    ┌──────────────────┐
   0 (highest)  │ SysTick          │──► timing / OS tick
   1            │ UART RX          │──► message receive
   2            │ Timer            │──► periodic publish
   3 (lowest)   │ PendSV           │──► background work
                └──────────────────┘
```

This is the most deterministic scheduling model — no jitter from software
task switching, no priority inversion, and worst-case latency is bounded by
the longest critical section that disables interrupts.

**Used by**: RTIC (exclusively), bare-metal interrupt handlers.

### Earliest Deadline First (EDF)

A dynamic-priority algorithm: the task with the nearest deadline always runs
next. EDF is **optimal** — it can schedule any task set that is schedulable
by *any* algorithm, up to 100% CPU utilization (vs. ~69% for RMA with
harmonic periods).

```
Time ──────────────────────────────────────►
        ┌───┐       ┌───┐
Task A  │ d=5│      │d=15│     (deadline 5, then 15)
        └───┘       └───┘
            ┌────┐      ┌────┐
Task B      │d=10│      │d=20│  (deadline 10, then 20)
            └────┘      └────┘
```

EDF is harder to analyze than FPP (no simple closed-form response time) and
harder to implement (priority changes every scheduling decision). Overload
behavior is also less predictable — under FPP, low-priority tasks miss
deadlines first; under EDF, deadline misses can cascade unpredictably.

**Used by**: Zephyr (`CONFIG_SCHED_DEADLINE` + `k_thread_deadline_set()`).
Not currently used by nano-ros.

## Platform Scheduling Comparison

### RTIC (ARM Cortex-M)

RTIC is not an RTOS — it is a concurrency framework that compiles directly
to hardware interrupt handlers. There is no scheduler, no task control
blocks, and no context switch overhead.

| Property | Value |
|----------|-------|
| **Model** | Interrupt-driven (NVIC hardware scheduling) |
| **Preemption** | Yes — hardware interrupt nesting |
| **Priority levels** | 4–16 (depends on Cortex-M variant) |
| **Priority direction** | Lower number = higher priority (ARM convention) |
| **Context switch** | 12 cycles (Cortex-M3 tail-chain) |
| **Mutual exclusion** | Stack Resource Policy (SRP) — compile-time deadlock-free |
| **Scheduling analysis** | Fully static — all priorities and resources known at compile time |

RTIC's key innovation is the **Stack Resource Policy (SRP)**: shared
resources are protected by ceiling-based priority elevation, not locks. The
compiler proves at build time that no deadlock or unbounded priority
inversion can occur. This gives interrupt-driven scheduling the safety of a
cooperative model.

```rust
#[rtic::app(device = stm32f4xx_hal::pac)]
mod app {
    #[task(priority = 2, shared = [sensor_data])]
    async fn read_sensor(mut ctx: read_sensor::Context) {
        ctx.shared.sensor_data.lock(|data| {
            *data = read_adc();  // runs at ceiling priority
        });
    }

    #[task(priority = 1, shared = [sensor_data])]
    async fn publish(mut ctx: publish::Context) {
        ctx.shared.sensor_data.lock(|data| {
            node.publish(*data);  // cannot deadlock — SRP guarantee
        });
    }
}
```

**nano-ros integration**: RTIC tasks call nano-ros directly — no executor
needed. Each RTIC task can own a publisher, subscription, or service handle.
See `examples/stm32f4/rust/zenoh/rtic-talker/`.

### FreeRTOS

The most widely deployed RTOS. Uses fixed-priority preemptive scheduling
with optional round-robin time-slicing for same-priority tasks.

| Property | Value |
|----------|-------|
| **Model** | Fixed-priority preemptive (FPP) |
| **Preemption** | Yes — `configUSE_PREEMPTION=1` (default) |
| **Priority levels** | `configMAX_PRIORITIES` (typically 8–32) |
| **Priority direction** | Higher number = higher priority |
| **Context switch** | ~80 cycles on Cortex-M (PendSV handler) |
| **Mutual exclusion** | Mutexes with optional priority inheritance |
| **Time slicing** | Optional — `configUSE_TIME_SLICING` |

FreeRTOS tasks are created with `xTaskCreate()`, specifying a priority and
stack size. The scheduler runs the highest-priority ready task. When
multiple tasks share a priority and time-slicing is enabled, they
round-robin at tick boundaries.

**Priority inheritance**: FreeRTOS mutexes optionally support priority
inheritance to mitigate priority inversion. When a low-priority task holds
a mutex needed by a high-priority task, the low-priority task temporarily
inherits the high priority until it releases the mutex.

```
Without inheritance:        With inheritance:
  H ──blocks──►             H ──blocks──►
  M ──runs────►             L ──promoted──► (runs at H's priority)
  L ──holds mutex           L ──releases──► H runs immediately
     (M preempts L →        (no M preemption → bounded inversion)
      unbounded inversion)
```

**nano-ros task layout** on FreeRTOS:

| Task | Default Priority | Stack | Role |
|------|-----------------|-------|------|
| nros_app | 3 (Normal) | 64 KB | Executor, callbacks, spin |
| net_poll | 4 (AboveNormal) | 1 KB | Poll LAN9118 RX FIFO |
| zenoh read | 4 (AboveNormal) | 5 KB | Socket read, message decode |
| zenoh lease | 4 (AboveNormal) | 5 KB | Keep-alive, lease monitor |
| tcpip_thread | 4 (AboveNormal) | 4 KB | lwIP protocol processing |

### ThreadX (Azure RTOS)

ThreadX is designed for deeply embedded systems with a unique
**preemption-threshold** feature not found in other RTOSes.

| Property | Value |
|----------|-------|
| **Model** | Fixed-priority preemptive (FPP) with preemption-threshold |
| **Preemption** | Yes — with configurable threshold per thread |
| **Priority levels** | 32 (0–31) |
| **Priority direction** | Lower number = higher priority |
| **Context switch** | ~60 cycles (optimized assembly for each architecture) |
| **Mutual exclusion** | Mutexes with priority inheritance |
| **Time slicing** | Per-thread configurable (`time_slice` parameter) |

The **preemption-threshold** is ThreadX's distinguishing feature. Each
thread has two priority values:

1. **Priority**: determines scheduling order (which thread runs next)
2. **Preemption-threshold**: the minimum priority that can preempt this thread

```
Thread A: priority=10, preempt_threshold=5
  → Scheduled based on priority 10
  → Only threads with priority 0–4 can preempt it
  → Threads with priority 5–9 must wait, even though they're higher priority

Thread B: priority=10, preempt_threshold=10  (threshold = priority)
  → Normal behavior — any higher-priority thread can preempt
```

This effectively creates **non-preemptive regions** without disabling
interrupts. A thread performing a critical sequence of operations can set a
low preemption-threshold to prevent most preemption while still allowing
the highest-priority threads through.

The academic basis is the **dual-priority model**: preemption-threshold
reduces context switches (and thus stack usage) while preserving
schedulability. Research shows it can reduce RAM requirements by 30–50%
compared to pure FPP, because fewer threads need independent stacks for
preemption frames.

**nano-ros on ThreadX**: Currently sets `preempt_threshold = priority`
(no benefit). Phase 76 will expose this as a configurable option.

### NuttX

NuttX is a POSIX-compliant RTOS, meaning it implements the full
`pthread` and `sched` APIs from IEEE 1003.1. This makes it the most
portable platform — standard POSIX real-time scheduling is well-understood
and widely taught.

| Property | Value |
|----------|-------|
| **Model** | POSIX SCHED_FIFO (FPP), SCHED_RR, or SCHED_SPORADIC |
| **Preemption** | Yes (FIFO/RR), configurable |
| **Priority levels** | 1–255 (POSIX `sched_param.sched_priority`) |
| **Priority direction** | Higher number = higher priority |
| **Context switch** | Kernel-managed, architecture-dependent |
| **Mutual exclusion** | POSIX mutexes with `PTHREAD_PRIO_INHERIT` or `PTHREAD_PRIO_PROTECT` |
| **Scheduling policy** | Per-thread via `sched_setscheduler()` |

NuttX supports three POSIX scheduling policies:

**SCHED_FIFO** (First-In-First-Out): Pure fixed-priority preemptive. A
task runs until it blocks, yields, or is preempted by a higher-priority
task. Tasks at the same priority run in FIFO order — no time-slicing.

**SCHED_RR** (Round-Robin): Same as SCHED_FIFO but with time-slicing for
same-priority tasks. Each task gets a time quantum before the scheduler
switches to the next task at that priority.

**SCHED_SPORADIC** (NuttX extension): Implements the **sporadic server**
algorithm for aperiodic event handling. A task alternates between a high
"normal" priority and a low "background" priority based on its execution
budget:

```
Budget = 5ms, Replenish period = 20ms

Time ──────────────────────────────────────────────────►
        ┌─────┐                    ┌─────┐
High    │5ms  │                    │5ms  │     (budget)
        └──┬──┘                    └──┬──┘
           │ ┌──────────────┐        │
Low        └►│ background   │        └►...    (budget exhausted)
             └──────────────┘
                    ▲ replenish after 20ms
```

The sporadic server bounds the interference that an aperiodic task imposes
on periodic tasks, making it analyzable with standard RMA techniques. This
is valuable for event-driven ROS callbacks that fire at irregular intervals.

**Priority inversion protocols**: NuttX supports both POSIX priority
inheritance (`PTHREAD_PRIO_INHERIT`) and priority ceiling
(`PTHREAD_PRIO_PROTECT`):

| Protocol | How it works | Trade-off |
|----------|-------------|-----------|
| **Priority inheritance** | Holder inherits waiter's priority | Dynamic — may chain through multiple locks |
| **Priority ceiling** | Mutex has fixed ceiling priority; holder runs at ceiling | Static — simpler analysis, avoids chained inversion |

**nano-ros on NuttX**: Currently runs with kernel defaults (no explicit
scheduling policy). Phase 76 future work will add `SCHED_FIFO` and priority
configuration.

### Zephyr

Zephyr provides the most flexible scheduling model, supporting cooperative
threads, preemptive threads, and EDF in a single system.

| Property | Value |
|----------|-------|
| **Model** | Cooperative + preemptive + optional EDF |
| **Preemption** | Configurable per-thread |
| **Priority levels** | Cooperative: `K_PRIO_COOP(0)` to `K_PRIO_COOP(N)` (negative values); Preemptive: `K_PRIO_PREEMPT(0)` to `K_PRIO_PREEMPT(N)` (positive values) |
| **Priority direction** | Lower number = higher priority |
| **Context switch** | Architecture-dependent (~100 cycles on Cortex-M) |
| **Mutual exclusion** | k_mutex with priority inheritance |
| **Meta-IRQ** | Ultra-high-priority threads that preempt even cooperative threads |

Zephyr's scheduling model has three tiers:

```
Priority Number Line:
◄── higher priority                    lower priority ──►
    │                │                    │
    │  Meta-IRQ      │  Cooperative       │  Preemptive
    │  (negative,    │  (negative,        │  (0 or positive)
    │   special)     │   non-preemptible) │
    │                │                    │
```

**Cooperative threads** (`K_PRIO_COOP`): Cannot be preempted by other
threads (only by interrupts). They run until they explicitly yield or
block. This is useful for critical sections that must not be interrupted
by other threads, while still allowing hardware interrupts.

**Preemptive threads** (`K_PRIO_PREEMPT`): Standard FPP behavior. Can
be preempted by any higher-priority thread (cooperative or preemptive).

**Meta-IRQ threads** (`CONFIG_NUM_METAIRQ_PRIORITIES`): Special
ultra-high-priority threads that can preempt even cooperative threads.
Used for work that must complete with interrupt-like urgency but needs
thread context (e.g., stack, blocking calls). This fills the gap between
ISR context (limited API) and thread context (preemptible).

**Deadline scheduling**: Zephyr optionally supports EDF via
`CONFIG_SCHED_DEADLINE`. Threads call `k_thread_deadline_set()` to
declare their next deadline. Among threads at the same priority level,
the scheduler picks the one with the earliest deadline. This allows EDF
within a priority band while preserving FPP across bands — a hybrid
approach that combines EDF's optimality with FPP's predictable overload
behavior.

**nano-ros on Zephyr**: Currently uses a single main thread with
default priority. The async service example uses Embassy's executor
with kernel-backed waking (`zephyr::embassy::Executor`).

## Priority Inversion

Priority inversion is a well-studied problem in real-time systems. It
occurs when a high-priority task is indirectly blocked by a low-priority
task through a shared resource, while a medium-priority task runs
unimpeded.

The classic example (from the Mars Pathfinder incident, 1997):

```
  High-priority task ──► blocks on mutex held by Low
  Medium-priority task ──► preempts Low (doesn't need mutex)
  Low-priority task ──► holds mutex, can't run (M preempts)

  Result: High is blocked by Medium indefinitely
```

### Solutions

| Solution | Approach | Platforms |
|----------|----------|-----------|
| **Priority inheritance** | Mutex holder inherits highest waiter's priority | FreeRTOS, ThreadX, NuttX, Zephyr |
| **Priority ceiling** | Mutex has fixed ceiling; holder runs at ceiling | NuttX (`PTHREAD_PRIO_PROTECT`) |
| **SRP (Stack Resource Policy)** | Compile-time ceiling, zero runtime overhead | RTIC |
| **Preemption-threshold** | Limit which tasks can preempt | ThreadX |
| **Lock-free design** | Avoid shared resources entirely | nano-ros single-slot buffers |

nano-ros mitigates priority inversion architecturally: subscriptions use
single-slot buffers with atomic overwrites — no mutex needed between
publisher and subscriber tasks. The executor processes callbacks in a
single task, eliminating inter-task resource sharing for most use cases.

## Choosing a Scheduling Model

| Criterion | RTIC | FreeRTOS | ThreadX | NuttX | Zephyr |
|-----------|------|----------|---------|-------|--------|
| **Determinism** | Best (hardware) | Good (FPP) | Good (FPP+threshold) | Good (POSIX FPP) | Good (FPP+coop+EDF) |
| **Worst-case latency** | 12 cycles | ~80 cycles | ~60 cycles | Kernel-dependent | ~100 cycles |
| **Priority inversion** | Impossible (SRP) | Inheritance | Inheritance + threshold | Inheritance + ceiling | Inheritance |
| **Analysis tools** | Compile-time proofs | RMA/RTA | RMA/RTA + threshold | POSIX standard RMA | RMA + EDF analysis |
| **Flexibility** | Low (ARM only) | Medium | Medium-High | High (POSIX) | Highest |
| **RAM overhead** | Lowest (no TCBs) | Low | Low (threshold reduces stacks) | Medium (kernel) | Medium (kernel) |

**Use RTIC** when: you need hard real-time on ARM Cortex-M, want
compile-time scheduling proofs, and can live without dynamic task creation.

**Use FreeRTOS** when: you need a widely supported RTOS with a small
footprint and a large ecosystem. Good for projects where portability across
MCU vendors matters more than advanced scheduling features.

**Use ThreadX** when: you need deterministic scheduling with reduced RAM
(preemption-threshold), or your project targets Azure IoT infrastructure.
ThreadX is also safety-certified (IEC 61508 SIL 4, ISO 26262 ASIL D).

**Use NuttX** when: you want POSIX compatibility (reuse Linux-targeted code
on embedded), need SCHED_SPORADIC for aperiodic events, or want
PTHREAD_PRIO_PROTECT for static priority ceiling analysis.

**Use Zephyr** when: you need maximum scheduling flexibility (cooperative +
preemptive + EDF in one system), want a Linux Foundation backed project with
broad hardware support, or need meta-IRQ for interrupt-like thread
priorities.

## Further Reading

- Liu, C.L. and Layland, J.W. (1973). "Scheduling Algorithms for Multiprogramming in a Hard-Real-Time Environment." *Journal of the ACM*, 20(1), 46–61. — The foundational paper for Rate-Monotonic Analysis.
- Sha, L., Rajkumar, R., and Lehoczky, J.P. (1990). "Priority Inheritance Protocols: An Approach to Real-Time Synchronization." *IEEE Transactions on Computers*, 39(9), 1175–1185. — Priority inheritance and priority ceiling protocols.
- Baker, T.P. (1991). "Stack-Based Scheduling of Realtime Processes." *Real-Time Systems*, 3(1), 67–99. — The Stack Resource Policy used by RTIC.
- Wang, Y. and Saksena, M. (1999). "Scheduling Fixed-Priority Tasks with Preemption Threshold." *RTCSA*. — The theoretical basis for ThreadX's preemption-threshold.
- Buttazzo, G.C. (2011). *Hard Real-Time Computing Systems: Predictable Scheduling Algorithms and Applications*. Springer. — Comprehensive textbook covering FPP, EDF, and hybrid approaches.
