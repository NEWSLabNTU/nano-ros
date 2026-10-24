---
id: 2018
title: "Kernel budget enforcement is not wired to levels (Linux SCHED_DEADLINE, NuttX SCHED_SPORADIC)"
status: resolved
type: enhancement
area: [executor, linux, nuttx]
severity: low
found: 2026-10-24
resolved: 2026-10-24
related: [phase-509, rfc-0108, 2016, 2017]
---

## Measured

phase-509 P10's last checklist item reads: "Kernel budget enforcement where
the kernel has it (Linux SCHED_DEADLINE, NuttX SCHED_SPORADIC), one PR per
platform, after its runner."

The runners have landed (#2548 native, #2553 FreeRTOS/NuttX, and the Rust
Zephyr/ThreadX PR). Budget enforcement has not:

- The NuttX C level runner calls `nros_nuttx_apply_current_sporadic` per level
  thread, as the per-tier runner did. The kernel therefore throttles the level
  thread. The executor's own budget accounting does not know about it.
- The native runner applies no `SCHED_DEADLINE`.
- On the C runners and on the Rust spawned levels,
  `apply_tier_sched_policy` (the executor's default scheduling context) is
  applied for the boot level only. A spawned level shares the core's executor,
  and that executor has one default context.

## Closes when

Each platform with kernel budgets ties a level's declared budget and period to
the level's thread, one PR per platform. The level's executor-side accounting
must agree with the kernel's. A test must show a level overrunning its budget
being throttled.

## Resolution

Two PRs, one per platform: NuttX (PR #2615) and Linux (this one).

**A level's own sched context.** Each level's tier policy (class, budget,
period, deadline) lowers through `SchedContext::from_tier_policy` onto a
context of the level's own, set after the level opens and before its setup
(`Executor::set_level_sched_context`; C ABI `nros_cpp_level_set_sched_policy`;
Rust `ExecutorNodeRuntime::apply_level_sched_policy`). What the setup
registers binds to it unless a node or group binding names another. This
replaces the boot level's policy on the executor's default context, which
every level's callbacks used to run on.

**Accounting agreement: one enforcer per level.** Where the kernel enforces
the level's budget on its thread, the executor hands it over
(`Executor::defer_budget_to_kernel`) and never skips the level for want of
budget. Where the kernel does not, the executor's cooperative budget enforces
the same numbers. The runner prints which (`level budget ... enforced by the
KERNEL|EXECUTOR`). Deferring was chosen over double accounting: two enforcers
of one budget throttle a level twice, on refill boundaries that do not line
up.

- **NuttX**: `SCHED_SPORADIC` on every budgeted level thread (C runner: every
  level; Rust arm: spawned levels, the session-owning boot level stays
  unbudgeted, #246). Both runners report a kernel throttle. Test:
  `sched_dims_applied_e2e::nuttx_level_budget_overrun_is_throttled_by_the_kernel`
  (QEMU, `ws-realtime-cpp` and `ws-realtime-rust`, whose `high` member
  overruns its 5 ms / 10 ms budget once in fifty ticks on NuttX): the kernel
  throttle, a later overrun, and no executor budget skip. Passes.
- **Linux**: `SCHED_DEADLINE` (`sched_setattr`) on every spawned budgeted
  level thread, both runners; the adopted session owner gets none. Without
  `CAP_SYS_NICE` the kernel refuses and the executor enforces, reported as the
  `SCHED_FIFO` fallback is. Tests: the reservation arithmetic, and a thread
  burning 20 ms of CPU at 2 ms per 10 ms, which must take at least 80 ms of
  wall time under the kernel.

## Not verified here

The Linux kernel-throttle half did not run on the host this was built on: it
has no `CAP_SYS_NICE`, and granting it needs root. The test checked the
fallback there (the reservation refused, the thread left off
`SCHED_DEADLINE`) and says on its output that the throttle half did not run.
It runs wherever the test binary has the capability.
