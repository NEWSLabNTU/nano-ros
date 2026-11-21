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

## Fix (not started)

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
