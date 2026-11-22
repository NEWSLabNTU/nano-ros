---
id: 2800
title: "ThreadX on Linux runs every thread on a host pthread stack: a host image cannot catch its target's stack overflow, and each pool-allocated stack is dead weight"
status: open
type: limitation
severity: low
area: [threadx, memory, build]
related: [issue-2732, issue-2400, phase-557]
found: 2026-11-22
---

## What

ThreadX's Linux port (`third-party/threadx/kernel`,
`ports/linux/gnu/src/tx_thread_stack_build.c`) creates each ThreadX thread
as a host thread:

```c
if(pthread_create(&thread_ptr -> tx_thread_linux_thread_id, NULL, _tx_linux_thread_entry, thread_ptr))
```

With NULL attributes, the thread runs on the host's DEFAULT pthread stack.
That stack's size comes from `RLIMIT_STACK` at run time, usually 8 MiB. The
stack handed to `tx_thread_create` only holds a fake stack pointer
(`tx_thread_stack_ptr = stack_end - 8`) and is never run on. The
threadx-linux board does not override this: no `pthread_attr_setstacksize`
or `pthread_attr_setstack` in the board or `nros-board-common/c`.

## Impact

1. **A host image cannot catch a stack overflow its target would hit.** The
   threadx-linux board is the host model of the ThreadX boards. A path that
   needs 200 KiB passes there, on a 64 KiB app stack, and overflows on
   `rv-virt-threadx`. For this reason the cargo road's stack-floor check
   reports threadx-linux images as not checked, rather than walking them
   against a stack nothing runs on (phase-557 W5b).
2. **Each thread's pool-allocated stack is dead weight in the byte pool.**
   `threadx_hooks.c` allocates the app thread's stack, and every sched
   task's, out of the byte pool. On this port those bytes are never used. So
   the pool's derived size (`app stack + WORKING_SET[rmw]`, issue 2624)
   counts stack the port does not use, and
   [issue 2732](2732-threadx-pool-counts-sched-task-stacks.md)'s
   `+ sched-task stacks` would count more of it. 2732's pool sizing should
   decide whether threadx-linux's derivation drops those terms, or keeps
   them so the host and target derive the same pool.

## Fix

Make the port run each thread on its ThreadX stack:
`pthread_attr_setstack(&attr, tx_thread_stack_start, tx_thread_stack_size)`
in `_tx_thread_stack_build`. That is a patch on the vendored ThreadX fork,
so it goes on the fork's patch-line branch and the maintainer pushes it.
Then:

- every threadx-linux image runs on the stack it states, so the host
  catches what the target would;
- the 64 KiB default app stack (`nros-board-threadx-linux/build.rs`) has to
  be re-measured for a host build, whose libc frames are larger than
  newlib's;
- the board states its app stack (`stack_stated::emit`), and threadx-linux
  moves from the not-checkable list to `STATING_PLATFORMS` in
  `packages/cli/nros-cli-core/src/cmd/stack_floor.rs`.

The stack must stay at or above the host's `PTHREAD_STACK_MIN`, which
differs by architecture (the "stack_bytes is a floor the port raises" rule).

## Progress (2026-11-22)

Done:
- The fork commit (`9fdb09be` on `nros-lp64-ulong`) runs each thread on its
  ThreadX stack, and the superproject pins it.
- One floor, `NROS_THREADX_HOST_STACK_MIN` + 128 in the board's
  `config/tx_user.h`, per architecture. TX_MINIMUM_STACK, the timer
  thread's stack and zenoh-pico's task stack read it. The board checks it
  against the running host's `sysconf(_SC_THREAD_STACK_MIN)` at init.
- Every image prints each thread's stack peak when it ends. The logging
  smoke reads: timer thread 4,840 of 16,512, app 7,240 of 65,536.
- The app stack resolves on the `app_stack_bytes` knob ladder, default
  64 KiB, and the board states it.

Open:
- The 64 KiB app stack, measured on networked images (2026-11-22, an aarch64
  host, b74a55df19, each Rust pair with its router, about 12 s of traffic,
  then SIGTERM, one run each): the app thread's peak is 45,888 B (listener and
  talker), 45,880 B (service server) and 45,904 B (service client). The timer
  thread's is 6,696 to 6,712 B. On that host the port raises both to 131,200,
  so the worst app peak is 35 % of the stack it ran on and 70 % of the stated
  64 KiB. Open: the C and C++ images, and an x86_64 host, have not been
  measured. The test harness ends images without the termination guard, so
  the cells print no peak lines; these were run by hand.
- `nros build`'s stack-floor walk does not reach a host image, and
  threadx-linux stays off `STATING_PLATFORMS`:
  [issue 2834](2834-host-image-stack-floor-walk.md).
- Issue 2732's pool question stands, now with stacks that are used.

