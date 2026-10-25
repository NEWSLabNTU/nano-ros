# The Real-Time Execution Model

This chapter is about one question: when does a nano-ros callback meet its
deadline, and what does the runtime do to make it? The design lives in
[RFC-0002](../../../docs/design/0002-rt-execution-model.md) (the executor's
scheduling contexts) and
[RFC-0108](../../../docs/design/0108-executor-preemption-levels.md) (executor
levels). [Scheduling Models](./scheduling-models.md) covers the RTOS
schedulers underneath.

## One level does not preempt

An executor runs a callback to completion. Within one OS thread nothing can
interrupt user code mid-callback, so a callback that becomes ready while
another runs waits for it to return. Its worst-case blocking is the longest
callback that can be running when it wakes:

```text
response time  >=  own execution time  +  longest other callback on the thread
```

A 5 ms telemetry callback on the same thread as a 1 kHz control loop makes that
loop miss its 1 ms deadline every time the two meet, whatever priority bucket
or scheduling class either has. The buckets and classes below change which
READY callback runs next. They cannot stop one that is already running.

## Scheduling classes

A callback runs in a scheduling context (`SchedContext`), and its class picks
how the executor orders ready work:

| Class | Orders ready work by | Use it for |
| --- | --- | --- |
| `Fifo` (default) | registration order within a priority bucket | most callbacks |
| `Edf` | absolute deadline (release + relative deadline) | periodic work with stated deadlines on one level |
| `Sporadic` | `Fifo`, plus a budget per replenishment period | a callback whose CPU share must be capped |
| `BestEffort` | last, after every other bucket | logging, telemetry |

A tier or level's `class = "real_time"` is the deployment-side spelling. On
Linux it also selects `SCHED_DEADLINE` for a level that states a budget (see
below). Time-triggered windows are a gate on a context
(`tt_window_offset_us` / `tt_window_duration_us`), not a class.

## Levels: preemption from the kernel

The fix for the bound above is a second thread. RFC-0108 places callbacks
on **executor levels**: one OS thread per level, all over one executor per
core. A more urgent level's thread has a higher kernel priority, so the
kernel preempts a less urgent level mid-callback when the urgent one wakes.

- **The number of levels is derived.** Each node's rank comes from its
  contract (`paths.<p>.trigger.timer.rate_hz`, rate-monotonic), and each
  distinct rank is one level, up to the image's `max_levels`
  (`CONFIG_NROS_EXECUTOR_MAX_LEVELS`, default 8).
- **Priorities are allocated, not authored.** A derived level takes a
  priority from the board's application band, below the transport tasks
  (RFC-0079). An authored `[tiers.<name>]` bound with `group_tiers` is a pin:
  its members share one level at its stated priority.
- **The boot level** is the least urgent one; it runs on `main()` and owns the
  session. The others are spawned.
- **On a multi-core host, levels run in parallel.** Preemption only shows when
  they compete for one CPU, which is why the Linux tests pin the process with
  `taskset -c 0`.

## Who enforces a budget

A level whose tier states a budget and a period has exactly one enforcer
(issue 2018). The kernel takes it where the kernel can; otherwise the
executor enforces the same numbers at callback boundaries. The runner prints
which:

```text
nros: level budget tier=`capped` 2000us/10000us enforced by the KERNEL
```

| Platform | Kernel mechanism | When the kernel enforces it |
| --- | --- | --- |
| Linux | `SCHED_DEADLINE` (runtime, deadline, period) | a spawned level with `class = "real_time"`, when the process holds `CAP_SYS_NICE` and the level is not pinned to fewer CPUs than its root domain |
| NuttX | `SCHED_SPORADIC` (`CONFIG_SCHED_SPORADIC`) | every budgeted level thread on the C runner; spawned levels on the Rust runner |
| Zephyr, FreeRTOS, ThreadX | none | never: the executor enforces it |

Without `CAP_SYS_NICE` a Linux image keeps running. The kernel refuses both
`SCHED_FIFO` and `SCHED_DEADLINE`, the image prints the refusal once with the
`setcap` command that fixes it, and every level runs at the default policy.
That image has levels but no preemption between them.

## Scenarios and what measured them

RFC-0002 §11 names twelve scenarios. Each row names the test that measured
it, or says that none has.

| Scenario | Status |
| --- | --- |
| S1, drone flight controller: 1 kHz critical loop beside 5 ms telemetry | Linux, on levels: `rt_sched_levels_e2e::s1_critical_loop_meets_every_deadline_on_its_own_level` (60 s, zero misses, plus a one-level negative control). It runs only on a lane that claims `rt-sched`. |
| S2, sense, plan and act at 10 Hz on one thread | No dedicated test. |
| S3, watchdog liveness | Levels feed the watchdog only when every level is alive (RFC-0108 D8). Measured on the S32K344 safety island in phase-509. |
| S4, Autoware-style mixed rates on Linux | No dedicated test. |
| S5, Cortex-M0+ sensor node | No dedicated test. |
| S6, `Trigger::AllOf` fusion | `trigger_conditions` (nros-tests) and the executor's unit tests. |
| S7, a 1 kHz subscription drained once a cycle | Idempotent ready-set insertion, unit-tested in `nros-node`. |
| S8, a long-running action goal | The action server is a cooperative state machine; covered by the action e2e tests, not by a timing test. |
| S9, a service handler that triggers a callback | No dedicated test. |
| S10, cross-level priority inversion | Priority-inheritance mutexes in the POSIX port (`c_port_posix`). No scenario test. |
| S11, several nodes on one executor | The default deployment; every multi-node example runs it. |
| S12, a cycle wall-time budget | Not built: there is no cycle budget option. |

## Running the real-time tests

Three tests need the kernel's real-time scheduling, and live in
`packages/testing/nros-tests/tests/rt_sched_levels_e2e.rs`:

- the preemption order of `levels-cpp`'s three levels on one CPU;
- a spawned level past its `SCHED_DEADLINE` budget, throttled by the kernel;
- S1, above.

No lane claims the `rt-sched` capability, so each is a `[SKIPPED:lane]`
everywhere by default. To run them on a host you control, build the fixtures,
grant the capability to the four `levels-cpp` binaries (it is bound to the
file's contents, so again after every rebuild), and claim the capability:

```sh
NROS_FIXTURE_ID=workspace-cpp-native-levels-s1 scripts/build/workspace-fixtures-build.sh linux cpp
# ... and -levels, -levels-s1-flat, -levels-budget
cd examples/workspaces/levels-cpp/build/posix-zenoh-linux/cmake
sudo setcap cap_sys_nice+ep linux_entry linux_s1_entry linux_s1_flat_entry linux_budget_entry
cd -
NROS_TEST_UNCLAIMED= cargo nextest run -p nros-tests --test rt_sched_levels_e2e
```

A run that claims `rt-sched` on a host without the capability fails with
`[UNMET PRECONDITION]` and names the binary to grant it to.
