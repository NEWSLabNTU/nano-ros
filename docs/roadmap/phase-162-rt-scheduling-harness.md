# Phase 162 — Real-time verification of executor levels on a privileged host

**Status (2026-10-25). Code landed; the acceptance run waits on a privileged
host.** W1–W3 are built: the `rt-sched` lane capability, three tests in
`packages/testing/nros-tests/tests/rt_sched_levels_e2e.rs` over three new
`levels-cpp` images, and the book chapter. On this host (no `CAP_SYS_NICE`,
`ulimit -r` 0) the tests skip by lane by default and fail `[UNMET
PRECONDITION]` when the scope is claimed, which is the designed behaviour; the
S1 negative control was seen to miss before that check. What is left is one
run on a host where an operator granted the capability (the chapter's
"Running the real-time tests"), and recording its numbers here. Every
self-hosted runner is `--cap-drop ALL`, so no lane can claim `rt-sched` until
one is started with `--cap-add SYS_NICE`.

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
`CAP_SYS_NICE` fails through `unmet!`. Then two tests in
`rt_sched_levels_e2e`, each asking the scope first:

- `linux_level_budget_overrun_is_throttled_by_the_kernel` runs `levels-cpp`'s
  `linux_budget` image: `load_capped` burns 10 ms of CPU per release on a
  spawned level with a 2 ms per 10 ms `SCHED_DEADLINE` reservation. It asserts
  the runner hands the budget to the KERNEL and that each burn takes several
  periods of wall time, most of it off the CPU. The unit test in
  `nros-board-linux` stays as it was: it asserts the fallback where the kernel
  refuses, and is not scoped.
- `linux_levels_preempt_in_rank_order_on_one_cpu` runs the three-level
  `linux` image under `taskset -c 0` and applies `level_order_verdict`, now in
  `nros_tests::levels`.
- An operator provides the lane: a runner started with `--cap-add SYS_NICE`,
  or `setcap cap_sys_nice+ep` on the fixture binaries of a local run. Nothing
  here may need `sudo` from the agent.

Two things the build found. A derived level is allocated from the top of the
app pool (`SCHED_FIFO` 88 on Linux), so a derived level beside an authored
`capped` tier at 80 outranked it, made `capped` the least urgent level and so
the boot level, which gets no reservation. The budget image therefore pins
its boot level too (`load_boot` on `background`). And `SCHED_DEADLINE`
refuses a thread pinned to fewer CPUs than its root domain, so the budget run
is not pinned.

**Done when** both tests pass on a lane that claims `rt-sched`, and every other
lane reports them as `[SKIPPED:lane]`.

### W2 — S1 on levels: zero deadline misses under load

RFC-0002 S1 on Linux levels, `levels-cpp`'s `linux_s1` image: `attitude`, a
1 kHz timer with 300 us of work and a 1 ms deadline, on its own derived level,
and `load`, S1's 5 ms best-effort telemetry at 10 Hz, on a less urgent one.
The negative control is `linux_s1_flat`, the same two classes bound to one
authored tier. `attitude` counts release gaps longer than period plus
deadline, a lower bound on deadline misses. The test
(`s1_critical_loop_meets_every_deadline_on_its_own_level`) pins both runs to
one CPU and is in scope for `rt-sched` only.

**Done when** 60 s of the run records zero misses on the urgent level, and the
negative control records misses. The second half shows the test can see a
miss. Unprivileged, pinned, both images miss (16–33 a second), as expected
with no preemption.

### W3 — the RT execution model chapter

`book/src/internals/rt-execution-model.md`: the non-preemption bound of one
level, the scheduling classes, how levels give preemption, which kernel
enforces a budget on each platform (issue 2018's table), and S1–S12 with each
one's measured status. Linked from
[phase-110](archived/phase-110-rt-execution-model.md)'s archive note.

**Done when** the chapter is in the rendered book, and each scenario row names
the test that measured it, or says it is unmeasured. Done on 2026-10-25.

## Acceptance

- [ ] W1: the kernel throttle and Linux level preemption pass on a lane that
      claims `rt-sched`, and skip by lane everywhere else.
- [ ] W2: S1 on levels, zero misses in 60 s, with the negative control.
- [x] W3: the chapter is published and phase-110's archive note points to it.

## Issues found here

Both came out of the first runs of the new images and are outside this
phase's scope. They are homed here so they have an owner.

- [Issue 2049](../issues/2049-cpp-levels-runner-info-lines-logged-as-errors.md):
  the C++ runners log three status lines at ERROR, so the boot report counts
  every multi-level boot as an error.
- [Issue 2050](../issues/2050-spin-quantization-warning-names-a-spawned-level-timer.md):
  the spin-quantization audit warns that `attitude`'s 1 ms timer runs in 10 ms
  bursts, which the run did not show. W2's privileged run tells which is wrong.

## Not in this phase

- Silicon measurements: [phase-512](phase-512-on-hardware-measurements.md).
- WCET numbers from a real run for phase-357's derivations: #259, homed in
  phase-460.
