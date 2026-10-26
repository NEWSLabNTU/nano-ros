---
id: 2066
title: "The Linux SCHED_DEADLINE level throttle has never run: no CI host has CAP_SYS_NICE"
status: open
type: tech-debt
area: [executor, linux, testing]
severity: low
found: 2026-10-26
related: [phase-509, rfc-0108, 2018]
---

## Measured

Issue 2018 (PR #2616) puts every budgeted spawned level thread on Linux under
`SCHED_DEADLINE` (`sched_setattr`). Without `CAP_SYS_NICE` the kernel refuses
the reservation and the executor enforces the budget instead.

The test burns 20 ms of CPU at 2 ms per 10 ms and requires at least 80 ms of
wall time under the kernel. It has run only without the capability. On
that path it checks the fallback (the reservation is refused and the thread
stays off `SCHED_DEADLINE`) and prints that the throttle half did not run.
Neither the development host nor CI has granted the capability, so these
have never been observed:

- the kernel throttling a level thread;
- `Executor::defer_budget_to_kernel` handing a level over with no executor
  budget skip.

NuttX's half (`SCHED_SPORADIC`) has a QEMU test that passes.

## Closes when

The throttle half runs and passes somewhere it can: a CI job with
`CAP_SYS_NICE` (a privileged container, or `setcap cap_sys_nice+ep` on the
test binary), or a recorded manual run. If neither is possible, the RFC-0108
docs must state that the Linux kernel throttle is unverified.
