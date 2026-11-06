# Pick a scheduling model

<!-- type: how-to · level: 3 -->

**Needs first:** [Organize a larger workspace](../build/organize-a-larger-workspace.md)

Pick a scheduling model when one image runs callbacks of different urgency: a
10 ms control loop beside a 100 ms telemetry publisher, for example, where the
control loop must not wait behind the telemetry.

In nano-ros, the unit of scheduling is a **sched task**: one executor running
on one RTOS thread, with its own priority and spin period, all of them sharing
one middleware session. You name callback groups in code, map each group to a
sched task in `system.toml`, and give each sched task its priority per RTOS.
The build turns that into the threads each platform has. Where ROS 2 reaches
for a `MultiThreadedExecutor` and callback groups, nano-ros gives you sched
tasks: an executor dispatches on one thread, and parallelism is a sched task,
not a callback group.

The scheduling theory behind the choice (fixed-priority preemption,
rate-monotonic assignment, EDF, priority inversion) belongs to
[The execution model](../../background/execution-model.md).

## Before you start

- A workspace whose image runs on an RTOS or on Linux.
- The capabilities your platform delivers, from the generated
  [Scheduling wiring matrix](../../reference/sched-matrix.md): kernel EDF on
  Zephyr, the sporadic server on NuttX, preemption threshold on ThreadX, core
  affinity on most.

## Steps

1. Name a callback group for each urgency, in the node's code.

   In Rust, call `node.callback_group("ctrl")?` before creating the timer
   or subscription that belongs to it. In C++, use the named form,
   `create_callback_group("ctrl")`; an unnamed group runs on the node's
   default sched task.

2. Map each group to a sched task, on the node's `[[component]]` row:

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

3. Declare the sched tasks. A key on `[tasks.<name>]` applies on every
   platform; a key on `[tasks.<name>.<rtos>]` applies only there:

   ```toml
   [tasks.high]
   class = "real_time"
   spin_period = "1000us"

   [tasks.high.posix]
   priority = 80
   core = 0

   [tasks.high.freertos]
   priority = 3

   [tasks.low]
   spin_period = "10000us"

   [tasks.low.posix]
   priority = 10

   [tasks.low.freertos]
   priority = 2
   ```

   `class` is `real_time` (the default for a priority sched task),
   `best_effort` or `time_triggered`. Priorities are the RTOS's own numbers,
   so their direction differs: on FreeRTOS, NuttX and Linux a bigger number
   is more urgent, and on ThreadX and Zephyr a smaller one is. The table of
   levels and directions per platform is in the
   [Scheduling wiring matrix](../../reference/sched-matrix.md).

4. Add the platform's own mechanism where it has one. Scope it to that
   platform, so it lowers to nothing elsewhere:

   ```toml
   [tasks.high.zephyr]
   priority = 9
   deadline = "10000us"        # kernel EDF, Zephyr only

   [tasks.high.nuttx]
   priority = 99
   budget = "5000us"           # SCHED_SPORADIC, NuttX only
   period = "10000us"

   [tasks.low.threadx]
   priority = 16
   preempt_threshold = 10      # ThreadX only
   time_slice = "5000us"
   ```

   A `budget` on `[tasks.high]` itself would switch every platform's executor
   to cooperative sporadic gating, which is why these keys go in the
   platform tables.

5. Keep sched tasks below the transport. Each board reserves a priority band
   for the middleware's own read and lease threads, and a sched task that
   outranks them cannot be served by them. The build allocates sched-task
   priorities from the board's application pool and refuses a priority that
   lands on the reserved band. A sched task that really must run above it
   says so with `above = "transport"`.

6. Choose what a periodic timer does after a stall. When a sched task is
   preempted past several periods, the timer fires once and counts the
   missed periods (`TimerOverrunPolicy::Skip`, the default, as in rclcpp and
   Zephyr's `k_timer`). Phase is kept, so activations stay on the declared
   cadence. Replay every missed period instead, for a counter or an
   accumulator, with
   `executor.set_timer_overrun_policy(id, TimerOverrunPolicy::CatchUp)`. Do
   not replay a control loop: the replayed activations compute with stale
   inputs and send a burst of commands. Watch `executor.timer_overruns(id)`,
   or the `timer-overrun-runtime` violation, rather than the publish rate,
   which looks nominal under `CatchUp` during exactly the stall it should
   catch. Timer periods are microseconds throughout, including across the C
   ABI.

7. Build. The configure runs the host image once to take its census, and
   each node logs the sched task it registered on:

   ```bash
   nros configure
   ```

   ```text
   [INFO] Telem::register on a sched task admitting group `telem`
   ```

## Example

[`examples/workspaces/realtime-rust/`](https://github.com/NEWSLabNTU/nano-ros/tree/main/examples/workspaces/realtime-rust)
runs a 10 ms control node on `high` and a 100 ms telemetry node on `low`, with
sched-task tables for Linux, FreeRTOS, NuttX, ThreadX and Zephyr in one
`system.toml`. Its `realtime-c` and `realtime-cpp` siblings do the same in C
and C++.

```toml
# src/demo_bringup/system.toml (excerpt)
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

[tasks.high]
class = "real_time"
spin_period = "1000us"
[tasks.high.posix]
priority = 80
core = 0
[tasks.high.freertos]
priority = 3

[tasks.low]
spin_period = "10000us"
[tasks.low.posix]
priority = 10
[tasks.low.freertos]
priority = 2

[image.linux]
board = "linux"

[image.freertos_realtime]
board = "freertos"
panic = "own"
locator = "tcp/10.0.2.2:7891"
```

```bash
nros run demo_bringup:linux
```

## If it doesn't work

- **The build refuses a priority.** It lands on the board's reserved
  transport band, or outranks it. Pick one from the application pool the
  message names, or state `above = "transport"`.
- **A `core` pin falls back with a warning on Zephyr.** The pin is on the
  boot sched task, which Zephyr started before nano-ros ran and cannot pin
  any more. Put `core` on a sched task nano-ros spawns.
- **`MultiThreadedExecutor` does not compile.** It has no nano-ros form. Move
  the callbacks that need their own thread to a sched task of their own.

## See also

- [Analyze timing](analyze-timing.md): check the callbacks a sched task runs
  for unbounded work.
- [Scheduling wiring matrix](../../reference/sched-matrix.md): what each
  platform delivers.
- [The execution model](../../background/execution-model.md) and
  [Dispatch strategy](../../background/dispatch-strategy.md): how executors,
  sched tasks and dispatch fit together.

## Design background

- [RFC-0015: RTOS orchestration](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0015-rtos-orchestration.md)
- [RFC-0053: ThreadX multi-tier execution](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0053-threadx-multi-tier-execution.md)
- [RFC-0079: priority is allocated, not authored](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0079-priority-is-allocated-not-authored.md)
- [RFC-0002: RT execution model (timer overrun policy)](https://github.com/NEWSLabNTU/nano-ros/blob/main/docs/design/0002-rt-execution-model.md)
