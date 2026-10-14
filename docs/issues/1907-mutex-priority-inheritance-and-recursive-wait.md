---
id: 1907
title: "POSIX and NuttX mutexes have no priority inheritance, RFC-0002 says
  NuttX does, and a condvar wait under a recursively held mutex deadlocks on
  three ports"
status: open
type: bug
area: [platform]
severity: medium
found: 2026-10-15
related: [issue-1196, issue-1906, rfc-0002]
---

**Home: none** — found by issue 1196's design study; no active phase owns the
platform lock contract.

## 1. No priority inheritance on POSIX or NuttX

`nros-platform-posix/src/platform.c`'s `nros_platform_mutex_init` calls
`pthread_mutex_init(m, NULL)`. The default protocol is `PTHREAD_PRIO_NONE`,
on glibc and on NuttX alike. The recursive mutex sets
`PTHREAD_MUTEX_RECURSIVE` and nothing more.

NuttX compiles the same file. It gives a default mutex priority inheritance
only when BOTH `CONFIG_PRIORITY_INHERITANCE` and
`CONFIG_PTHREAD_MUTEX_DEFAULT_PRIO_INHERIT` are set
(`libs/libc/pthread/pthread_mutexattr_init.c`). None of the three defconfigs
under `packages/boards/nros-board-nuttx-qemu/nuttx-config/` sets either one
(checked: no `PRIORITY_INHERITANCE` in any of them).

RFC-0002's capability table (`docs/design/0002-rt-execution-model.md`, the
"PI mutex" row) marks NuttX "✅ default w/ `CONFIG_PRIORITY_INHERITANCE`".
That is wrong twice: it is not the default, and it is not configured.

Issue 1196 exempted mutex waits from its "no unbounded wait" rule on the
grounds that every port has priority inheritance. Zephyr, FreeRTOS and ThreadX
do. POSIX and NuttX do not, so on those two a priority inversion through a
nano-ros mutex is unbounded.

**Fix direction.** Set `PTHREAD_PRIO_INHERIT` on both mutex kinds in the POSIX
port (Linux supports it on every glibc we target), and enable
`CONFIG_PRIORITY_INHERITANCE` in the NuttX defconfigs. Then correct the
RFC-0002 row.

## 2. A condvar wait under a recursively held mutex deadlocks

On Zephyr, FreeRTOS and ThreadX, a condvar wait releases the mutex ONCE:

- Zephyr: `k_condvar_wait` calls `k_mutex_unlock` once
  (`zephyr/kernel/condvar.c:118`).
- FreeRTOS and ThreadX: our ports call `nros_platform_mutex_unlock(m)` once,
  and both build every mutex as a recursive one.

If the caller holds `m` at depth 2 or more, the mutex stays held across the
"wait". Every signaller then blocks on `m`, and that is a deadlock. A timed
wait does not rescue it, because the waiter never released the lock. NuttX is
correct: `pthread_cond_clockwait` uses `pthread_mutex_breaklock`, which
releases every level and restores it after.

The FreeRTOS port's own comment says zenoh-pico holds `_z_mutex_t`
recursively on its read-task paths. No measured deadlock is known.

**Fix direction.** State the contract in `nros/platform.h`: a condvar wait
requires `m` held exactly once. Then either assert it in debug builds (the
kernels expose the owner's lock count), or release and restore every level,
as NuttX does.
