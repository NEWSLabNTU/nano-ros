---
id: 2934
title: "On a host that may not request real-time scheduling, a POSIX task that states a priority never starts"
status: open
type: bug
severity: medium
area: [platform, posix, linux]
related: [issue-0636, issue-0765, issue-0803, issue-2866]
found: 2026-11-24
---

## What happens

`nros_platform_task_init` on the POSIX port maps any band priority (0..255)
to `SCHED_FIFO`. Since issue 0636 it sets that on the thread ATTRIBUTE, with
`PTHREAD_EXPLICIT_SCHED`, so the task is born with it. On a host without
`CAP_SYS_NICE` or an `RLIMIT_RTPRIO` allowance (every unprivileged `just ci`
run, and every CI runner container, which drops all capabilities), the
kernel refuses that at CREATE time. `pthread_create` returns EPERM, the port
returns `NROS_PLATFORM_RET_NOMEM`, and the task never exists.

The port already had the rule for this case, written before 0636: a refused
priority is reported once and the task runs at the inherited one
("turning a missing capability into a boot failure would make every
unprivileged `just ci` run stop dead"). That arm runs AFTER `pthread_create`,
so after 0636 it was unreachable for exactly the case it was written for.
zpico's transport task relies on it too (its comment names "the EPERM arm").

Found while writing issue 2866's board re-ask task. Its first cut asked for
the least-urgent band value, and on this host the task never started. That
cut now inherits the creator's priority, which is a workaround at one site.

## Sweep

Every task spawn on a hosted POSIX system goes through this one function:
the platform ABI's callers, zpico's transport (through
`nros_platform_task_attr_t`) and the clock re-ask helper. The Linux board
applies a sched task's priority to the RUNNING thread
(`nros_posix_apply_current_priority`), which already reports a refusal and
never stops the task. The NuttX board's own explicit-`SCHED_FIFO` spawns
(`nuttx_run_sched_tasks.c`) run on NuttX, which has no privilege model, so
they cannot meet EPERM. Swept with:

    git grep -n "PTHREAD_EXPLICIT_SCHED\|pthread_attr_setinheritsched" -- packages ':!*third-party*'

## Fix

When an explicit-scheduling create returns EPERM, the port retries once
with the creator's scheduling (stack size and detach state kept). The
post-create arm then asks for `SCHED_FIFO` on the running thread again, is
refused, and reports it once, as it was written to.

## Acceptance

`tests/posix-task-priority-tests.sh` (`just check posix-task-priority`) runs
the real port with `RLIMIT_RTPRIO` at 0. It first proves that `SCHED_FIFO` is
refused; a process that could still get it is an unmet precondition, never a
pass. Then:

- two band-priority tasks spawn and run;
- the refusal is reported once, naming EPERM;
- an inheriting task is unchanged.

Negative control: the port without the retry fails to spawn.
