# Phase 162 — Real-time verification of executor levels on a privileged host

**Status (2026-10-25). PARKED, rescoped.** Not started. Rewritten around
RFC-0108's executor levels, which replaced the multi-executor design this phase
was first written against. W1 and W2 need a host or runner with
`CAP_SYS_NICE`. This host has none (no capabilities, `ulimit -r` 0, a
`PREEMPT_DYNAMIC` kernel, not `PREEMPT_RT`), and every self-hosted runner is
`--cap-drop ALL`. W3 is a writing task and can start now. Unpark W1 and W2 when
an operator provides a privileged lane.

**Implements:** [RFC-0002](../design/0002-rt-execution-model.md) scenarios S1
and S3, on [RFC-0108](../design/0108-executor-preemption-levels.md) levels.

**Prior:** [phase-110](archived/phase-110-rt-execution-model.md) (runtime and
API), [phase-509](archived/phase-509-executor-preemption-levels.md) (levels),
[issue 2018](../issues/archived/2018-level-kernel-budget-enforcement.md)
(kernel budgets on levels).

---

## What changed on 2026-10-25

The phase as first written (2026-05, re-audited 2026-09-06) planned a
standalone harness around `Executor::open_threaded` and
`SchedPolicy::Deadline`, a NuttX bench crate, and a PiCAS item. Measured
against the tree on 2026-10-25:

| Old item | Now |
| --- | --- |
| 162.A.1–A.2: `nros-rt-harness` crate and `just rt-test` | **Dropped.** Levels run as ordinary images, and their tests are ordinary tests. `SchedPolicy::Deadline` has no hits in the tree. What is missing is a lane with the capability, not a harness (W1). |
| 162.A.3: drone S1, zero deadline misses under load | **Kept** as W2, on levels. |
| 162.A.4: watchdog S3, multi-executor preemption | **Mostly met.** phase-509 feeds the watchdog only when every level is alive (RFC-0108 D8), and `realtime_tiers_e2e` reads preemption order off the console for the levels cells. On Linux, tier priority preemption is still advisory, because `SCHED_FIFO` needs the capability too. That half moves to W1. |
| 162.A.5: Linux `SCHED_DEADLINE` budget enforcement | **Built** by issue 2018. `nros-board-linux`'s `a_level_past_its_deadline_budget_is_throttled_by_the_kernel` exists, but on an unprivileged host it checks only the fallback and prints that the throttle half did not run. W1. |
| 162.B: NuttX `SCHED_SPORADIC` bench crate | **Done another way.** Issue 2018 put `SCHED_SPORADIC` on every budgeted NuttX level thread. `sched_dims_applied_e2e::nuttx_level_budget_overrun_is_throttled_by_the_kernel` passes under QEMU. |
| 162.C: Cortex-M3 wake latency on silicon | **Moved** to [phase-512](phase-512-on-hardware-measurements.md) H4 on 2026-10-24. |
| 162.D: PiCAS evaluation | **Dropped.** `OsPrioritySet` is gone from the tree, and levels are the dispatch model. |
| 162.E: book chapter | **Kept** as W3. |

The old test design would also break today's rule: it planned to print
`[SKIPPED]` on stock CI. Issue 1758 requires the lane to declare its scope
instead, and a claimed capability that is absent to fail.

## Work items

### W1 — a lane that claims real-time scheduling, and the tests that need it

Add a `Capability::RtScheduling` (token `rt-sched`) to
`nros_tests::lane_scope`, shaped like `HostBridge`: no lane claims it today,
an unclaimed run deselects through `lane_skip!`, and a claimed run without
`CAP_SYS_NICE` fails through `unmet!`. Then:

- `a_level_past_its_deadline_budget_is_throttled_by_the_kernel` reaches the
  scope check first. Out of scope, it skips. In scope, a refused reservation
  is a failure, not a printed note. The fallback assertion stays as its own
  test, which runs everywhere.
- A Linux levels cell asserts preemption order, the way the levels cells do
  with `level_order_verdict`, in scope only.
- An operator provides the lane: a runner started with `--cap-add SYS_NICE`
  (and `--ulimit rtprio=99`), or `setcap cap_sys_nice+ep` on the test binary
  of a local run. Nothing here may need `sudo` from the agent.

**Done when** both tests pass on a lane that claims `rt-sched`, and every other
lane reports them as `[SKIPPED:lane]`.

### W2 — S1 on levels: zero deadline misses under load

RFC-0002 S1 on Linux levels: a 1 kHz critical timer (0.8 ms WCET, 1 ms
deadline) on the urgent level, and a best-effort level that keeps the CPU busy
with 5 ms callbacks. In scope for `rt-sched` only.

**Done when** 60 s of the run records zero deadline misses on the urgent level,
and the same image with both levels at one priority records misses. The second
half is the negative control: it shows the test can see a miss.

### W3 — the RT execution model chapter

`book/src/internals/rt-execution-model.md`: the non-preemption bound of one
level, how to choose a `SchedClass`, how levels give preemption, which kernel
enforces a budget on each platform (issue 2018's table), and S1–S12 with each
one's measured status. Link it from
[phase-110](archived/phase-110-rt-execution-model.md)'s archive note.

**Done when** the chapter is in the rendered book, and each scenario row names
the test that measured it, or says it is unmeasured.

## Acceptance

- [ ] W1: the kernel throttle and Linux level preemption pass on a lane that
      claims `rt-sched`, and skip by lane everywhere else.
- [ ] W2: S1 on levels, zero misses in 60 s, with the negative control.
- [ ] W3: the chapter is published and phase-110's archive note points to it.

## Not in this phase

- Silicon measurements: [phase-512](phase-512-on-hardware-measurements.md).
- WCET numbers from a real run for phase-357's derivations: #259, homed in
  phase-460.
