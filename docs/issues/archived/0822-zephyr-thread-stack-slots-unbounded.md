---
id: 822
title: "zenoh-pico's Zephyr port hands out thread stacks past the end of a fixed
  4-entry array once an image has created more than four tasks"
status: resolved
type: bug
area: rmw
resolved_in: efb227c7
related: [issue-0821]
---

## What was wrong

`src/system/zephyr/system.c` picked each task's stack with an ever-rising
counter over a fixed array:

```c
K_THREAD_STACK_ARRAY_DEFINE(thread_stack_area, Z_THREADS_NUM /* 4 */, ...);
static int thread_index = 0;
...
(void)pthread_attr_setstack(&tmp, &thread_stack_area[thread_index++], ...);
```

`thread_index` is never reset and never bounds checked. The **fifth** task an
image ever creates gets a stack pointer one whole stack past the end of the
array, and every task after that reaches further into whatever `.bss` follows.
Nothing reports it: the threads run with their stacks overlapping live data.

## Why it went unnoticed

Four slots is enough for a steady session — one read task, one lease task. It
is a **reconnect** that spends them: each re-open creates two more, so the
second reconnect is already out of bounds. An image that never loses its
session never reaches the bug.

## Resolution

Replaced with a claim/release slot table (`efb227c7` on
`jerry73204/zenoh-pico`). Running out is now a clean
`_Z_ERR_SYSTEM_TASK_FAILED` instead of silent corruption.

**Release is on `join`, not on `free`**, and that distinction is load-bearing.
`_z_transport_clear` DETACHES both tasks when tearing down from inside one of
them (`detach_tasks == true`, the lease-expiry path) and then calls
`_z_task_free`. A detached thread may still be running at that moment, so
releasing its slot there would hand a live thread's stack to the next task — a
worse bug than the one being fixed. `pthread_join` returning is proof the
thread is gone.

A claimed slot also has its previous occupant's tid zeroed before the lock
drops, and the matcher skips that placeholder, so a reserved-but-not-yet-owned
slot cannot be matched and freed by a concurrent join of an unrelated task
that was handed the same tid value.

**Known residue, not papered over:** a detach-teardown still retires its slot
for the life of the image, so a repeatedly-reconnecting image eventually runs
out and `_z_task_init` starts failing. Cleanly, but it fails. Fixing that needs
a thread-exit hook this API does not have.

## Scope — this did not fix the fault it was found under

Found while chasing
[issue 0821](../0821-zenoh-pico-faults-at-lease-expiry-on-zephyr.md) (a USAGE
FAULT at exactly `2 x Z_TRANSPORT_LEASE`). It is a genuinely separate defect
that the same reconnect loop would have reached shortly afterwards — 0821
still reproduces with this fixed, with slots to spare and no exhaustion
diagnostic.

## Upstream

`fix/zephyr-thread-stack-slots` on `jerry73204/zenoh-pico`, branched from
`e621319b` (== `eclipse-zenoh/zenoh-pico` main, 1.10.0), one file, ready to
open as a PR.
