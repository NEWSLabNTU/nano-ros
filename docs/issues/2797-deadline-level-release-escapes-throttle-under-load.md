---
id: 2797
title: "linux_level_budget_overrun_is_throttled_by_the_kernel fails on a loaded host: now and then one SCHED_DEADLINE release burns its 10 ms unthrottled"
status: open
type: bug
severity: medium
area: [linux, scheduling, testing]
found: 2026-11-21
related: [2018, 2066]
---

**Owner:** unassigned.

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
