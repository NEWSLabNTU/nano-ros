---
id: 2797
title: "linux_level_budget_overrun_is_throttled_by_the_kernel fails on a loaded host: now and then one SCHED_DEADLINE release burns its 10 ms unthrottled"
status: resolved
type: bug
severity: medium
area: [linux, scheduling, testing]
found: 2026-11-21
resolved: 2026-11-21
resolved_in: "fix(#2797): the budget burn outruns two budgets at the lowest clock, as the kernel charges a budget in work"
related: [2018, 2066]
---

**Owner:** unassigned (resolved).

## What happens

`linux_level_budget_overrun_is_throttled_by_the_kernel` (`rt_sched_levels_e2e.rs`)
runs the levels-cpp budget image: the `capped` level holds a SCHED_DEADLINE
reservation of 2000 us per 10000 us and burns 10 ms of CPU per release. The
kernel accepts the reservation (`nros: deadline budget set sched_task=`capped`
2000us/10000us`), and most releases throttle as they should: 30-50 ms of wall
time, with 7-10 ms off the CPU waiting for a refill. But one or two releases in
about 30 do not throttle at all:

| Release | cpu_us | wall_us | max_gap_us |
| ---: | ---: | ---: | ---: |
| 3 | 10000 | 32553 | 8218 |
| **4** | 10000 | **11737** | **1737** |
| 5 | 10000 | 50025 | 8471 |
| **16** | 10000 | **11028** | **1028** |

`throttle_verdict` requires every release after the first to wait off the CPU
for a quarter of a refill, so one such release fails the test.

Measured on 2026-11-21 on a 24-CPU host at load average 18-20 (other sessions'
builds), kernel 6.8.0-138-generic, `CONFIG_HZ=1000`, `CONFIG_SCHED_HRTICK=y`:
**7 runs, 7 failures**, the unthrottled release at a different index each time
(1, 2, 4, 5, 6, 11, 13). Earlier the same day, at lower load, the test passed.

It is not the transport placement in the same branch: with the C/C++ runner's
transport placement disabled (the band never set), it failed 4 of 4 runs in the
same way.

## What is known

- The reservation carries no flags (`sched_flags = 0`, so no
  `SCHED_FLAG_RECLAIM`), so GRUB reclaiming idle bandwidth is not the
  explanation.
- The run is not pinned to one CPU (`run_levels_image(.., false, ..)`), so the
  task can migrate under global EDF.
- An unthrottled release takes 10-12 ms of wall time for 10 ms of CPU, so it ran
  its whole burn in one stretch.

## What closes it

- Find the mechanism. Candidates to rule out: migration during a burn
  replenishing the budget, and the CBS wakeup rule resetting the deadline when
  the release starts late on a loaded host. A `sched_switch` /
  `sched_stat_runtime` trace of one failing run would show which.
- Then either the runner (a fix to how the reservation is set) or the verdict
  (if the kernel's behaviour is correct and the test's model is wrong), never a
  looser threshold without the mechanism.

## Resolved (2026-11-21): the kernel is right, the verdict's model was wrong

**Mechanism.** Linux charges a SCHED_DEADLINE runtime in time at the CPU's
maximum capacity, not in CPU time. Without `SCHED_FLAG_RECLAIM`,
`update_curr_dl_se` scales every slice it charges by
`arch_scale_freq_capacity` and `arch_scale_cpu_capacity`. So a CPU clocked
down to a fifth of its maximum burns a 2 ms budget in 10 ms of CPU, and a
10 ms burn on it never throttles. The host runs the `powersave` governor
(intel_pstate, Core Ultra 7 270K Plus).

**Measured.** A temporary probe in `load_pkg` read the thread's own
`/proc/thread-self/sched` before and after each release: `se.sum_exec_runtime`
(the CPU time it really used), `dl.runtime` and `dl.deadline` (the budget the
kernel still held, and how far it moved the deadline, one period per refill)
and `se.nr_migrations`. The probe ran over 6 runs and 168 releases at load 19-20,
and 2 runs failed.

| Release | real CPU | wall | refills | `dl.runtime` before -> after | charged / real |
| --- | ---: | ---: | ---: | --- | ---: |
| 25 (throttled) | 9.86 ms | 31.6 ms | 3 | 1.99 -> 0.16 ms | 0.79 |
| **26 (free)** | 8.99 ms | 9.9 ms | **0** | 1.96 -> 0.06 ms | **0.21** |
| 27 (throttled) | 10.05 ms | 40.1 ms | 4 | 2.00 -> 2.00 ms | 0.80 |

The free release stayed on one CPU (no migration and no involuntary switch),
and the policy stayed `SCHED_DEADLINE` (6). Across all 168 releases the
kernel charged a median 0.87 of the real CPU time, and the three releases that
never left the CPU had the three lowest ratios (0.21, 0.23 and 0.27).

What was ruled out:
- Migration. The free release did not move.
- The CBS wakeup rule. The deadline did not move at all.
- RECLAIM. `sched_flags = 0`.

One oddity is left unexplained. With the probe reading `/proc` every 1 ms
DURING the burn, 12 of 12 runs passed at load 34-39, against 5 of 6 failing
without it. A syscall-heavy burn evidently ran clocked up. It was not
pursued, since the mechanism above does not depend on it.

**Confirmed against the clock.** A second probe also read the CPU's
`scaling_cur_freq` after each release. Over 224 releases at load 14-16:
- The 52 releases that never left the CPU ran at a median 2.27 GHz and were
  charged a median 0.26 of their CPU time.
- The ones that throttled ran at 4.70 GHz and were charged 0.71.
- The lowest charge was 0.13, which is the hardware's floor: 0.8 GHz against
  6.8 GHz is 0.118.

One run had 14 of its 28 releases free. So how many releases run free is not
bounded by any threshold, and a majority rule (tried first) still flaked: 1
run in 10.

**Fix: size the burn, not the threshold.**
- **The burn.** A release can reach at most two budgets without waiting: what
  is left of the current one, plus one immediate refill when its deadline has
  already passed. It is charged at least `burn x min_clock / max_clock`. So a
  burn charged more than two budgets at the lowest clock throttles on every
  release. `load_capped` now burns 80 ms against 2 ms per 10 ms, which is
  charged at least 9.4 ms on the measured host. That is 2.4 times the 4 ms that
  two budgets cover.
- **The precondition.** The e2e reads the host's own clock range
  (`cpuinfo_min_freq`/`cpuinfo_max_freq` over every CPU) and refuses to judge,
  with `unmet!`, when that range admits a free release
  (`deadline_burn_margin`).
- **The verdict.** `throttle_verdict` requires every release after the first to
  wait off the CPU for a refill, as it did before.
- **The run.** The window is 6 s, because a release now takes ~250 ms of wall
  time.

The mean wall-time floor that issue 2066 introduced is retired. It assumed a
CPU-time model, which no clock-dependent charge satisfies. It also had a
rounding cliff: `cpu_us` 10003 rounds the period count from 4 to 5, which moves
the floor from 20 ms to 25 ms.

The book's execution-model chapter now says a budget is an amount of work.
