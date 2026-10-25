# Phase 162 — Real-time verification of executor levels on a privileged host

**Status (2026-10-25). Complete; archived.** The `rt-sched` lane
capability, three tests in
`packages/testing/nros-tests/tests/rt_sched_levels_e2e.rs` over three new
`levels-cpp` images, and the book chapter are built. The capability comes
from a container, not from `sudo`: the tests' default launcher runs each image
in a throwaway `docker run --cap-add SYS_NICE --ulimit rtprio=99`, which needs
only `docker` group membership, and host-tests' `rt-sched` job grants the same
to its job container. All three pass both ways (numbers under W1 and W2). The
two issues the build found are homed in
[phase-516](../phase-516-low-severity-issues-with-no-home.md) W6 and W7.

**Implements:** [RFC-0002](../../design/0002-rt-execution-model.md) scenarios S1
and S3, on [RFC-0108](../../design/0108-executor-preemption-levels.md) levels.

**Prior:** [phase-110](phase-110-rt-execution-model.md) (runtime and
API), [phase-509](phase-509-executor-preemption-levels.md) (levels),
[issue 2018](../../issues/archived/2018-level-kernel-budget-enforcement.md)
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
| 162.C: Cortex-M3 wake latency on silicon | **Moved** to [phase-512](../phase-512-on-hardware-measurements.md) H4 on 2026-10-24. |
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
- The capability comes from the launcher, `NROS_RT_SCHED_LAUNCHER`. `docker`
  (the default) runs each image in a throwaway container started with
  `--cap-add SYS_NICE --ulimit rtprio=99`, sharing the host's network and
  CPUs, with only the fixture's directory mounted read-only. Its image is the
  host's own distribution and release (`ubuntu:22.04` on jammy), or
  `NROS_RT_SCHED_IMAGE`. `direct` runs the binary as it is, for a process that
  already holds the capability: host-tests' `rt-sched` job, whose job
  container is granted `SYS_NICE` and runs as root, or a `setcap`'d fixture.
  Nothing needs `sudo`. The CI job is the only lane that claims `rt-sched`.

Two things the build found. A derived level is allocated from the top of the
app pool (`SCHED_FIFO` 88 on Linux), so a derived level beside an authored
`capped` tier at 80 outranked it, made `capped` the least urgent level and so
the boot level, which gets no reservation. The budget image therefore pins
its boot level too (`load_boot` on `background`). And `SCHED_DEADLINE`
refuses a thread pinned to fewer CPUs than its root domain, so the budget run
is not pinned.

**Done when** both tests pass on a lane that claims `rt-sched`, and every other
lane reports them as `[SKIPPED:lane]`.

Measured 2026-10-25 with the docker launcher (48-CPU host, generic
6.8.0-138 kernel with `PREEMPT_DYNAMIC`, load average 8-15 from other work):
both pass. The budget run printed ``deadline budget set tier=`capped`
2000us/10000us`` and ``enforced by the KERNEL``, and each 10 ms burn took
22.7-32.1 ms of wall time with 6.8-8.0 ms stretches off the CPU: the kernel's
throttle, which the executor's own enforcement cannot produce. With
`rt-sched` unclaimed all three are `[SKIPPED:lane]`.

On GitHub (host-tests run 40164384448, `ubuntu-22.04`, 4 vCPUs, the job
container granted `SYS_NICE`, launcher `direct`): all three pass. The capped
level's burns took 50.0-50.2 ms of wall time each, with 8.0-8.3 ms stretches
off the CPU.

### W2 — S1 on levels: deadlines kept under load

RFC-0002 S1 on Linux levels, `levels-cpp`'s `linux_s1` image: `attitude`, a
1 kHz timer with 300 us of work and a 1 ms deadline, on its own derived level,
and `load`, S1's 5 ms best-effort telemetry at 10 Hz, on a less urgent one.
The negative control is `linux_s1_flat`, the same two classes bound to one
authored tier. `attitude` counts release gaps longer than period plus
deadline, a lower bound on deadline misses. The test
(`s1_critical_loop_meets_every_deadline_on_its_own_level`) pins both runs to
one CPU and is in scope for `rt-sched` only.

**Done when** 60 s of the run keeps the urgent level's deadlines, and the
negative control records misses, which shows the test can see a miss. "Keeps"
depends on the kernel (`s1_verdict` in `nros_tests::levels`):

- On a `PREEMPT_RT` kernel (`/sys/kernel/realtime` is 1), zero misses.
- On a generic kernel, a miss rate at least 50 times below the negative
  control's. Zero is not a property any program has there: a bare C loop at
  the same `SCHED_FIFO` 88, on the same CPU, with no nano-ros and no load,
  missed 2 of 60,000 releases (worst gap 2.6 ms) on CPU 0 and 0 on CPU 23.
  What the levels own is that telemetry no longer runs in front of the
  critical loop, and that is what the bound measures.

Measured 2026-10-25, same host as W1, pinned to CPU 0, six runs: the
negative control missed 128-181 of 6,000-7,000 releases (about 2 %, worst gap
5.4-7.4 ms, one telemetry burn), and S1 missed 0-5 of 59,000 (under 0.01 %,
worst gap 1.4-3.4 ms). The first S1 assertion was `misses == 0`; it failed on
5 of 6 runs, and the bare C loop is what showed the misses were the host's,
not the levels'. On GitHub's 4-vCPU runner (the same run as W1) the control
missed 88 of 7,000 and S1 1 of 59,000 (worst gap 2.1 ms). Unprivileged, both images miss 16-33 a second, as expected
with no preemption.

### W3 — the RT execution model chapter

`book/src/internals/rt-execution-model.md`: the non-preemption bound of one
level, the scheduling classes, how levels give preemption, which kernel
enforces a budget on each platform (issue 2018's table), and S1–S12 with each
one's measured status. Linked from
[phase-110](phase-110-rt-execution-model.md)'s archive note.

**Done when** the chapter is in the rendered book, and each scenario row names
the test that measured it, or says it is unmeasured. Done on 2026-10-25.

## Acceptance

- [x] W1: the kernel throttle and Linux level preemption pass on a run that
      claims `rt-sched`, and skip by lane everywhere else.
- [x] W2: S1 on levels, with the negative control: zero misses on a
      `PREEMPT_RT` kernel, the host's floor on a generic one.
- [x] The host-tests `rt-sched` job runs green on GitHub: run 40164384448,
      2026-10-25, 4 vCPUs, all three passed (none skipped).
- [x] W3: the chapter is published and phase-110's archive note points to it.

## Issues found here

Both came out of the first runs of the new images and are outside this
phase's scope. They were homed here while the phase was open, and moved to
[phase-516](../phase-516-low-severity-issues-with-no-home.md) (W6, W7) when it
was archived.

- [Issue 2049](../../issues/archived/2049-cpp-levels-runner-info-lines-logged-as-errors.md):
  the C++ runners log three status lines at ERROR, so the boot report counts
  every multi-level boot as an error.
- [Issue 2050](../../issues/archived/2050-spin-quantization-warning-names-a-spawned-level-timer.md):
  the spin-quantization audit warns that `attitude`'s 1 ms timer runs in 10 ms
  bursts. W2's run under `SCHED_FIFO` showed the warning is the wrong half.

## Not in this phase

- Silicon measurements: [phase-512](../phase-512-on-hardware-measurements.md).
- WCET numbers from a real run for phase-357's derivations: #259, homed in
  phase-460.
