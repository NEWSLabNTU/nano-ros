---
id: 2796
title: "The POSIX port never names a thread it spawns, because its pthread_setname_np call is behind _GNU_SOURCE and no build defines it"
status: open
type: bug
severity: low
area: [linux, posix, platform, scheduling]
found: 2026-11-21
related: [2617, 2792]
---

**Owner:** unassigned.

## What happens

`nros_platform_task_create` in `packages/platform/nros-platform-posix/src/platform.c`
names the new thread with `pthread_setname_np`, but only under `#ifdef
_GNU_SOURCE`. The file deliberately compiles under `_POSIX_C_SOURCE` +
`_DEFAULT_SOURCE` (its own comment says so), and `git grep GNU_SOURCE` finds no
build that defines it: not a `build.rs`, not a cmake file. So the `#else` arm
always compiles, and every thread spawned through the platform task ABI keeps
the process's name.

The C/C++ native runners spawn every level and sched task that way
(`PlatformTask::spawn_with` with `nros-level-<name>` / `nros-sched-task-<name>`).
The boot thread census of the levels-cpp image shows every thread as
`linux_entry`:

| Thread | Policy | Priority |
| --- | --- | ---: |
| `linux_entry` | OTHER | 0 |
| `linux_entry` | OTHER | 0 |
| `linux_entry` | FIFO | 89 |
| `linux_entry` | FIFO | 88 |

The Rust board's census does show `nros-sched-task`, but only because that
runner spawns through `std::thread::Builder::name`, not through the port.

What it costs: `ps -L`, `top -H`, a crash dump and the census cannot tell the
levels apart, and the census test has to accept "the binary's name" as one of
ours instead of matching a level by name.

## What closes it

- The port names its threads on Linux. Either declare `pthread_setname_np`
  where the header withholds it (the file already does this for
  `sem_clockwait`), or call `prctl(PR_SET_NAME)` from the trampoline. The name
  is cut at 15 bytes either way.
- The census test then identifies the C/C++ levels by name.
