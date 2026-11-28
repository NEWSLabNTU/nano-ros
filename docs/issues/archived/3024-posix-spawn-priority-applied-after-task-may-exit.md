---
id: 3024
title: "The POSIX port sets a task's priority after the task may already have exited, and then reports the wrong refusal"
status: resolved
type: bug
severity: medium
area: [platform, posix, scheduling]
found: 2026-11-28
resolved: 2026-11-28
resolved_in: "fix(posix): a task applies its own priority and name before its entry runs"
---

## What happens

`nros_platform_spawn` in `packages/platform/nros-platform-posix/src/platform.c`
creates the thread, then calls `pthread_setschedparam(*t, SCHED_FIFO, …)` on it
("belt and braces", for a host that declined the creation-time attribute). The
thread can be detached (`PTHREAD_CREATE_DETACHED`), and it runs as soon as it is
created. A task that finishes before the creator gets to that call has already
exited. Linux then returns `ESRCH` for its id, and calling a pthread function on
an exited, detached thread's id is undefined.

The refusal reporter that follows is once per process. So on an unprivileged
host, a task that exits early spends the only report on:

    [warn] nros: SCHED_FIFO priority 49 was refused (rc=3) — the sched task runs at its inherited priority.

and the report that matters, `REFUSED (EPERM)` naming `setcap cap_sys_nice`, is
never printed for the process.

Observed 2026-11-28: `just check posix-task-priority` failed inside a loaded
`just check fast` with "the refused priority was reported 0 times, not once",
its stderr holding the `rc=3` line above. It passes alone (3 of 3). The test
counts `REFUSED (EPERM)`, so it is right to fail: the image said the wrong thing.

## Why

The priority is applied by the creator to another thread whose lifetime the
creator does not control. The start-up check at `platform.c`'s
`pthread_setschedparam(pthread_self(), …)` already shows the safe shape: a
thread applying its own scheduling.

## Fix direction

Apply the post-create priority from inside the task, before its entry runs: a
start trampoline that calls `pthread_setschedparam(pthread_self(), …)` and does
the read-back and the once-only report, then calls `entry(arg)`. That way the id
is always live, the report names the real errno, and a task that exits
immediately still reports a refused priority correctly. Keep the creation-time
attribute and its EPERM retry as they are.

Test: make the probe's task exit immediately, under contention (several spawns
in a row), and assert `REFUSED (EPERM)` is reported exactly once and `rc=3`
never.

## Resolution

`nros_platform_task_init` now starts every task through
`nros_posix_task_trampoline`. A heap start record carries the entry, its
argument, the native priority and a copy of the name, because the caller's
attribute need not outlive `pthread_create`. The task names itself, applies its
own priority with `pthread_setschedparam(pthread_self(), …)`, does the
read-back and the once-only report, frees the record, and then calls the entry.
The creator never touches the new thread's id again. The creation-time
attribute and its EPERM retry are unchanged, and the once-only reporter is
atomic, since tasks now report from their own threads. Thread naming had the
same shape (`pthread_setname_np(*t, …)` after create) and moved with it.

Sweep of the other ports: FreeRTOS, ThreadX and Zephyr pass the priority at
creation (`xTaskCreate`, `tx_thread_create`, a pthread attribute), and every
post-create priority change in the boards is a task changing its OWN
(`vTaskPrioritySet(NULL, …)`, `tx_thread_priority_change(self, …)`,
`pthread_setschedparam(pthread_self(), …)` on NuttX,
`k_current_get()` on Zephyr). NuttX runs this port, so it gets the fix.

Test: `tests/posix-task-priority-tests.sh` case d compiles the port with
`pthread_create` wrapped so the creator resumes only after the child has
exited, which makes the race deterministic. Against the unfixed port it fails
exactly as the gate did ("refused (rc=3)", EPERM reported 0 times). Against
the fixed port EPERM is reported once and nothing else is.

