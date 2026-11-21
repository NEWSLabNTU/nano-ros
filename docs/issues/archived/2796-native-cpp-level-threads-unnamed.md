---
id: 2796
title: "The POSIX port never names a thread it spawns, because its pthread_setname_np call is behind _GNU_SOURCE and no build defines it"
status: resolved
type: bug
severity: low
area: [linux, posix, platform, scheduling]
found: 2026-11-21
resolved: 2026-11-21
resolved_in: "fix(#2796): the POSIX port names the threads it spawns, and the runners name their levels to fit"
related: [2617, 2792]
---

**Owner:** unassigned (resolved).

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

## Resolved (2026-11-21)

- The port names its threads on glibc, keyed on the C library as
  `sem_clockwait` is: the prototype is supplied when the header withholds it.
  The name is cut at 15 bytes in the port, because glibc REFUSES a longer one
  with ERANGE instead of cutting it, which would have left every long-named
  thread unnamed even with the call compiled in. musl, NuttX and the BSDs still
  spawn unnamed, as before.
- The cut made the old prefixes useless: `nros-sched-task-` is 16 bytes and
  `nros-level-` 11, so the level's own name was lost or cut to four letters.
  Every runner (both C/C++ spawns and the Rust board) now names a level
  `nros-<label>`, where the label (`nros::thread_name::label`) is the sched
  task's name less the derivation's `derived-` prefix, which every derived
  level carries. The levels-cpp census now reads `nros-urgent` and
  `nros-middle`, where it read `nros-derived-ur` and `nros-derived-mi` with
  the prefix kept. That also helps FreeRTOS, whose default
  `configMAX_TASK_NAME_LEN` is 16.
- The census test identifies a level by that name, and fails when no
  SCHED_FIFO level carries one.

zenoh-pico's transport tasks are still unnamed: they are spawned by
zenoh-pico's own POSIX system layer, not through the port.
