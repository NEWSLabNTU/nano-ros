# The execution model

<!-- type: explanation · level: 3 -->

The real-time example workspace runs a 10 ms control node and a 100 ms
telemetry node in one image, each on a sched task of its own:

```toml
[[component]]
pkg = "ctrl_pkg"
class = "ctrl_pkg::Control"
name = "control_node"
group_tasks = { ctrl = "high" }

[[component]]
pkg = "telem_pkg"
class = "telem_pkg::Telem"
name = "telem_node"
group_tasks = { telem = "low" }
```

When the configure takes the image's census, each node logs where it landed:

```text
[INFO] Telem::register on a tier admitting group `telem`
```

Why two sched tasks, and not one executor that runs both nodes? This page
answers that question: how an executor runs callbacks, how the two ways of
receiving data differ, where preemption comes from, and how the RTOS
underneath schedules the threads nano-ros creates. The task itself is
[Pick a scheduling model](../guide/realtime/pick-a-scheduling-model.md).

## A callback runs to completion

An executor runs one callback at a time, to completion. Within one thread
nothing interrupts your code mid-callback, so a callback that becomes ready
while another runs waits for it to return. Its worst case is its own
execution time plus the longest other callback that can be running when it
wakes:

```text
response time  >=  own execution time  +  longest other callback on the thread
```

Put the 1 ms control loop and a 5 ms telemetry callback on one thread, and
the loop misses its deadline every time the two meet, whatever priority or
class either has. Priorities and classes decide which *ready* callback runs
next. They cannot stop one that is already running. Preemption needs a
second thread, which is what a sched task is.

## Two ways to receive

Every entity (a subscription, a service, a client, an action server or
client, a timer) can be used through one of two layers, with separate verbs,
so the choice is visible in the source:

| | You poll | The executor dispatches |
| --- | --- | --- |
| Rust verbs | `Node::create_polling_subscription`, `create_polling_service`, `create_client`, `create_publisher` | `Executor::register_subscription`, `register_service`, `register_timer`, `register_action_server`; or rclrs's `Node::create_subscription(topic, callback)` |
| You get | an owned handle you `take()` from, or a `Promise<T>` | a callback that runs when data, a request or a timer is ready |
| Who schedules | your loop or your RTOS task | `spin_once` |

The polled constructors are named `create_polling_*` because rclrs's
`create_subscription` takes a callback, and a polled form under the same
name would compile a ported line into a different contract.

Poll when your code already owns its schedule (a bare-metal loop, one
FreeRTOS task per entity) or when the flow is request and response: a
service client's `call` returns a `Promise<Reply>`, and an action client's
`send_goal` leads to `get_result`. Typed service and action clients have only
the polling form. Dispatch when the code is callback-shaped, the way rclcpp
code is: one closure per entity, all routed by one `spin_once`. A dispatched
callback and its captures are stored in the executor's arena, whose size the
build sets; a polled handle needs no arena entry.

Both layers share one session and mix freely in one node. C and C++ split
them by type: `rclc_polling_subscription_t` and the `rclcpp::Poll*`
templates are the polling forms, beside the dispatched `rcl_subscription_t`
and `rclcpp::Subscription`. A polled handle can also take a wake callback,
so an RTOS task sleeps until data lands instead of polling; a backend that
cannot wake falls back to polling. The signatures are in the
[Rust](../reference/rust-api.md), [C](../reference/c-api.md) and
[C++](../reference/cpp-api.md) API references.

## What one spin does

`spin_once(timeout)` waits for I/O, up to the timeout, in the platform's own
wait primitive, so the thread sleeps rather than spins. When I/O is ready it
drains all of it, runs the ready callbacks, and then the expired timers and
guard conditions. The backend may report its own next internal event (a
lease keep-alive, a heartbeat), and the wait is capped at that.

Draining everything is the right default for one task doing all the ROS
work: it gives the most throughput. Its cost is that a timer can wait
behind every subscription callback that was ready before it. `SpinOptions`
bounds a blocking `spin`:

| Option | Effect |
| --- | --- |
| `max_callbacks(n)` | return after `n` callbacks, so a loop re-checks timers between them |
| `timeout(d)` | return after `d`, for a cyclic frame that gives ROS a fixed slot |
| `spin_once()` | one round of work, then return |
| `poll_interval(d)` | how long one pass may wait on the transport (default 10 ms) |

With `max_callbacks(1)` a timer waits behind one callback, not all of them.
With `timeout(d)` the work left over resumes next frame; a single callback
longer than the slot still overruns it. Neither option bounds a callback
itself, so a hard deadline still needs its own thread.

## Sched tasks and levels: preemption from the kernel

A **sched task** is one executor on one RTOS thread, with its own priority
and spin period. All of an image's sched tasks share one middleware session.
A more urgent task's thread has a higher kernel priority, so when it wakes
the kernel preempts a less urgent one mid-callback. That is the second
thread the bound above asks for.

You don't have to write sched tasks yourself. The build derives **levels**
from the contract: each node's rank comes from its trigger rate (rate
monotonic: a faster timer is more urgent), and each distinct rank is one
level, up to a cap that depends on the platform. When the ranks outnumber
the cap, adjacent levels merge, and the build reports each node that lost
its own rank. A sched task you declare in `[tasks.<name>]`, bound to a
callback group with `group_tasks`, is a pin: its members share one level at
its stated priority, and derivation places only the nodes no task binds.

Three more rules shape what you see:

- **Priorities are allocated.** A level takes its priority from the board's
  application band, below the threads the transport runs for itself. A
  priority that would land in that band is refused, unless the task says
  `above = "transport"`.
- **The boot level is the least urgent.** It runs on `main()` and owns the
  session; the build spawns the others.
- **On a multi-core host, levels run in parallel.** Preemption shows only
  when they compete for one CPU.

On Linux, the 1 kHz-loop-beside-5 ms-telemetry case is measured: on its own
level, the loop misses no deadline in 60 s on a `PREEMPT_RT` kernel, and on
one level it does.

### Inside a level: scheduling classes

Within a level, a callback's scheduling class orders the ready work:

| Class | Orders ready work by | Use it for |
| --- | --- | --- |
| `Fifo` (default) | registration order within a priority bucket | most callbacks |
| `Edf` | absolute deadline | periodic work with stated deadlines on one level |
| `Sporadic` | `Fifo`, plus a budget per replenishment period | a callback whose CPU share must be capped |
| `BestEffort` | last, after every other bucket | logging, telemetry |

A sched task's `class` (`real_time`, `best_effort` or `time_triggered`) is
the deployment-side spelling.

### Who enforces a budget

A level whose sched task states a budget and a period has exactly one
enforcer. The kernel takes it where it can; otherwise the executor enforces
the same numbers at callback boundaries. The image prints which:

```text
nros: level budget tier=`capped` 2000us/10000us enforced by the KERNEL
```

On Linux the kernel enforces it with `SCHED_DEADLINE`, for a spawned
`real_time` level, when the process holds `CAP_SYS_NICE`. On NuttX it is
`SCHED_SPORADIC`. Zephyr, FreeRTOS and ThreadX have no kernel budget, so the
executor enforces it. Without `CAP_SYS_NICE`, a Linux image keeps running:
it prints the refusal once, with the `setcap` command that fixes it, and
every level runs at the default policy. That image has levels but no
preemption between them.

## The RTOS underneath

Each level is an ordinary RTOS thread, and the RTOS schedules it. All of the
supported kernels schedule by **fixed-priority preemption**: the
highest-priority ready thread runs, and a more urgent thread that becomes
ready preempts it at once. Rate-monotonic assignment, which the level
derivation uses, gives shorter periods higher priority, and it is optimal
among fixed-priority assignments for independent periodic tasks. A task's
worst-case response time under it is:

```text
R_i = C_i + Σ_j ⌈R_i / T_j⌉ × C_j      (over every higher-priority task j)
```

where `C` is a worst-case execution time and `T` a period. The task meets
its deadline when `R_i ≤ D_i`.

**Earliest deadline first** (EDF) runs the task with the nearest deadline. It
can schedule any task set up to 100% utilization, where rate-monotonic
assignment is guaranteed only to about 69% in general, but it is harder to
analyze and less predictable in overload. nano-ros has EDF as the executor's
`Edf` class on every platform, and Zephyr also has it in the kernel.

**Round-robin** shares the CPU in time slices among threads of the *same*
priority. It prevents starvation, at the price of jitter. FreeRTOS slices
equal priorities by default, so two levels at one priority there would buy no
preemption, only memory, and the build merges them.

Each kernel has its own mechanism beyond this, which you reach through a
per-RTOS table of a sched task:

- **ThreadX preemption threshold:** a thread may be preempted only by threads
  above its threshold, which makes a non-preemptible region without masking
  interrupts.
- **NuttX `SCHED_SPORADIC`:** a sporadic server that caps an aperiodic task's
  CPU share by a budget per period.
- **Zephyr:** cooperative threads (preempted only by interrupts), preemptive
  threads, and kernel EDF among threads of one priority.

Priority numbers run in opposite directions: on FreeRTOS, NuttX and Linux a
bigger number is more urgent, and on ThreadX and Zephyr a smaller one is.
What each platform delivers is generated in the
[Scheduling wiring matrix](../reference/sched-matrix.md).

### Priority inversion

A high-priority task that waits on a lock held by a low-priority one can be
held up indefinitely by a medium-priority task that preempts the holder. The
usual cures are priority inheritance (the holder runs at the waiter's
priority), which every supported kernel offers on its mutexes, and priority
ceilings (the holder runs at the lock's fixed ceiling), which NuttX offers.
The cheaper cure is to share nothing between levels: give each level its own
data, and pass values between them as messages.

## Deferred dispatch

By default a callback runs **inline**: from the executor's spin, on the
thread that drives the transport. A node declares otherwise through
`Node::DISPATCH`, a `DispatchStrategy`:

- **`Inline`** (the default) is served on every platform.
- **`Deferred`** queues the callback for a dispatch task, which decouples its
  latency from network polling. The entries generated for the supported
  boards have no dispatch task, since each sched task's loop is already the
  only task that runs its callbacks, so a `Deferred` node runs inline there.
  It still builds unchanged for an entry that does drain the queue.
- **`FromIsr`** is reserved and refused by `nros check`: no generated entry
  dispatches from an interrupt handler.

The strategy is per node, not per callback, so a node is wholly one or the
other. A deferred callback runs on a different task from the one that
registered it, so its state cannot live in a closure's captures without a
heap. A `Deferred` node therefore registers with the `_static` forms
(`create_subscription_static`) and gets a tag, a `&'static str` identifier.
Its state lives in the node's `State`, and its `on_callback` matches the tag
against the event. An inline closure's captures live on the spin task's
stack frame, no heap, no boxing, so closures stay on the inline path; the
macro refuses a closure on a `Deferred` node.

The queue under `Deferred` is the executor's own, and you can use it without
the macros: `set_deferred` marks a callback, `set_defer_notifier` says which
one is queued, and `run_deferred` or `run_deferred_handle` runs it from a
task whose priority nano-ros owns. The queue is one bit per callback slot,
so it allocates nothing, and a timer cannot be deferred.

## What nano-ros does not run on

Every shape on this page keeps one promise: nano-ros decides which callback
runs and at what priority. A loop owned by a foreign async runtime or event
loop (tokio, Embassy, Qt, GLib) would decide that instead, so no real-time
bound could be stated, and nano-ros does not support one. The same holds for
a hand-written drain loop on a thread nano-ros did not create.

## Timing evidence

nano-ros states no measured worst-case execution times. The analysis it
supports is static: you check the callbacks a sched task runs for unbounded
loops, recursion and missing timeouts with Clippy and a call-stack tool
([Analyze timing](../guide/realtime/analyze-timing.md)), and the level
derivation uses execution times where a callback record carries
one. Certification evidence (ISO 26262, DO-178C) needs a static WCET tool on
the final binary: aiT from AbsInt is the commercial standard, and Platin is
an open-source option that works on LLVM output.

## Where nano-ros differs from ROS 2

- **No `MultiThreadedExecutor`, and callback groups do not make threads.** An
  executor dispatches on one thread. Parallelism is a sched task, and a
  callback group only names which sched task its callbacks run on.
- **Threads and priorities are generated.** The build creates the RTOS
  threads, allocates their priorities from the board's band and spawns them
  from the generated entry. You state rates and tasks, not thread code.
- **The spin is a poll loop.** There is no wait set: `spin_once` drives I/O,
  and its wait is bounded by `poll_interval`.
- **Polled entities are first-class.** ROS 2 receives through callbacks;
  nano-ros also has `create_polling_*`, under a different name.

The [ROS 2 executor documentation](https://docs.ros.org/en/humble/Concepts/Intermediate/About-Executors.html)
describes the model nano-ros departs from.

## Design background

- [RFC-0002](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0002-rt-execution-model.md):
  the executor's scheduling contexts, and the twelve scenarios the model
  answers.
- [RFC-0108](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0108-executor-preemption-levels.md):
  preemption levels, how they are derived and the budget enforcer.
- [RFC-0079](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0079-priority-is-allocated-not-authored.md):
  priority is allocated, not authored.
- [RFC-0113](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0113-rt-unified-wait.md):
  every supported way to run nano-ros keeps the real-time promise; the
  dispatch strategies.
- [RFC-0053](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0053-threadx-multi-tier-execution.md):
  ThreadX multi-tier execution.
