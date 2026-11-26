---
id: 2959
title: "The baremetal mps2 latency cells went red in the nros12 release tier 2 from the run's own QEMU guests and host load, not from the code"
status: resolved
resolved: 2026-11-26
resolved_in: "#4835 (the isolation); this change records the acceptance"
type: bug
severity: high
area: [ci, testing, qemu]
related: [issue-2540, issue-2706, phase-590, phase-591]
found: 2026-11-24
---

**Home:** [phase-590](../../roadmap/phase-590-nros12-release-plan.md), release
blocker B31.

## What happened

The nros12 release dispatch's tier 2 (run 45422927491, `730dfbebeb`, the
`newslab-118-server` runner) failed two real tests and nothing else:

| test | message |
| --- | --- |
| `emulator::test_qemu_rtic_levels_preempt_and_wake_on_time` | only 716 of 758 idle-time activations within 500 us |
| `isr_wake_rtos::a_timer_interrupt_wakes_the_parked_executor_on_baremetal_mps2` | only 47 of 50 callbacks within 500 us of their interrupt |

Both are the IDLE-time share each test allows QEMU (95 %): while the guest
idles in `wfi`, `-icount sleep=on` runs its clock at wall speed, so host
scheduling delays reach the sample. Both cells passed in tier 2 and the
pairwise nightly at the previous head that morning (`0d9fa3e470`, run
45351389552), and both passed again in the same release run's pairwise
nightly an hour later, on the same commit and runner.

## Measured

**The failing run's own distribution.** The runner kept the RTIC cell's
console log (`build/failed-run-logs/`). Of 1,003 activations:

| | n | p50 | p90 | p95 | p99 | max | within 500 us |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| during burns (the design bound) | 245 | 197 | | | | 199 | 245 |
| idle (the QEMU allowance) | 758 | 223 | 243 | 557 | 2,326 | 3,864 | 716 |

The design bound held for every sample. Only the idle tail moved, which is
a host-scheduling signature, not a slower wake path.

**What ran beside it.** 60 other tests overlapped the RTIC cell's 9 s in
that job, several of them QEMU guests (`test_qemu_bsp_pubsub_e2e`,
`test_qemu_rtic_service_e2e`, the AN536 Cyclone entry, `entry_matrix`), and
30 overlapped the ISR cell's 2 s. No other CI job shared the host: 118 has
one runner. But the host itself was busy with **local work by an agent
session on that host**: the pre-push hooks of several pushes (each runs the
~665 gate tests of `check fast` in parallel) overlapped the cells at
16:29Z. Nothing in the job can see that load, which is why it reads as a
code red.

**Solo, at both heads, under the host's typical background.** Ten rounds of
each cell, one at a time, in worktrees at `730dfbebeb` and at `0d9fa3e470`,
on newslab-118 while other users' work kept running (load average 13 to 29
at the release head, 8 to 13 at the earlier one; runner idle):

| head | ISR on-time share | ISR samples p50 / p99 / max (us) | RTIC worst during burns | RTIC idle on-time share |
| --- | --- | --- | --- | --- |
| `730dfbebeb` (release) | 50/50 in all 10 | 189 / 241 / 313 | 199 us | 755/757 to 758/758 (>= 99.7 %) |
| `0d9fa3e470` (last green) | 50/50 in all 10 | 185 / 241 / 247 | 198 to 199 us | 756/757 to 759/759 (>= 99.9 %) |

No regression between the heads. Under load 29, solo, the idle share stays
at 99.7 % or more against the 95 % allowance.

## The bound, checked against what the tests claim

The design's bound is 500 us, held where the guest's clock is its own (the
burns, and the ISR wake path). Measured: 199 us worst, 2.5x inside it, in
every run including the failed one. It is not widened. The idle allowance
(95 %) is QEMU's, and solo it holds with a wide margin at the host's
typical load. What it cannot absorb is a run that puts dozens of other
tests, QEMU guests among them, on the same cores at the same moment.

## Fix

`.config/nextest.toml`: the latency cells (`binary(isr_wake_rtos)` and the
RTIC levels cell) take `threads-required = "num-test-threads"`, so nextest
runs each alone in the run: it starts only when nothing else is running,
and nothing starts beside it. They take about 10 s each.

That isolates them from the run, not from the host. Two rules cover the
rest:

- **Read host load first.** When a wall-clock latency cell goes red on a
  self-hosted runner, check what else the HOST was doing (other users'
  jobs, and agent sessions' gates, fixture builds and pre-push hooks on that
  host) before calling it a regression.
- **Quiet hosts for release evidence.** The nros12 re-cut runs its lanes in
  a window in which hosts 118 and 241 run no agent gates, fixture builds,
  pre-push hooks or pushes (phase-590).

## Acceptance

Tier 2 green on main in the re-cut's evidence run, with both cells run
under the new isolation.

## Resolved (2026-11-26): acceptance met

The release dispatch at `01388e07a3` (run 45684820840) ran both cells under
the #4835 isolation on the 118 runner, and they passed in both of its tier-2
jobs:

| Cell | tier 2 (job 139454519217) | tier-2 nightly (job 139498579863) |
| --- | --- | --- |
| `emulator::test_qemu_rtic_levels_preempt_and_wake_on_time` | PASS 9.6 s | PASS 9.7 s |
| `isr_wake_rtos::a_timer_interrupt_wakes_the_parked_executor_on_baremetal_mps2` | PASS 2.3 s | PASS 2.3 s |

Tier 2's verdict job reads "cells ran and passed". The nightly's red that run
was B42 (issue 2975, `test-ignored`), not these cells.

The later reds on these cells in the tier-2 nightly of 2026-11-25 12:50Z were
release blocker B38: wfi wake latency on an idle host, a different cause. B38
merged as #4866. The two rules above (read host load first; quiet hosts for
release evidence) stay in force.
