---
id: 2784
title: "No sched task's stack is checked against its walked floor, on any RTOS: only the thread an image boots on is"
status: open
type: limitation
severity: medium
area: [memory, stack-floor, freertos, nuttx, zephyr, threadx]
related: [2400, 2732, phase-557]
found: 2026-11-21
---

## What

The stack-floor check (issue 2400, phase-557) walks an image's deepest path from
the thread it BOOTS on and compares the floor with that thread's stack:

| Platform | Checked thread | Its stack |
| --- | --- | --- |
| FreeRTOS | the boot app task (`~app_task_entry_`) | `app_stack_bytes` |
| NuttX | the init task (`nros_nuttx_entry`) | the kernel's `CONFIG_INIT_STACKSIZE` |
| Zephyr | the main thread | `CONFIG_MAIN_STACK_SIZE` |
| bare metal | `main` | `NROS_BAREMETAL_STACK_KB` |

A **sched task** is a different thread, with a stack of its own, and no
platform checks it:

- **FreeRTOS** — `[tasks.*.freertos] stack_bytes`.
- **NuttX** — a pthread per task, `[tasks.*.nuttx].stack_bytes`, else
  `NROS_NUTTX_SCHED_TASK_STACK_BYTES` (16 KiB,
  `packages/boards/nros-board-nuttx-qemu/c/nuttx_run_sched_tasks.c`).
- **Zephyr** — `CONFIG_NROS_ZEPHYR_SCHED_TASK_STACK_SIZE` (16 KiB by default).
- **ThreadX** — taken from the byte pool (issue 2732 is about sizing that pool,
  not about checking the stack against a walk).

The callbacks a sched task runs are where an image's per-message work happens,
which is the deepest code most images have. An image whose sched-task stack is
below what its callbacks use builds green and overflows at run time, where the
failure is a corrupt neighbour or a hang rather than a message. That is the
failure the check exists to prevent, on exactly the threads it does not reach.

## Why it is not a small addition

Each role needs three things the boot thread already has:

1. **A root.** The boot thread has one named entry. A sched task's entry is the
   board's task trampoline, which reaches the callbacks of that task's
   executor only through the dispatch table, so the walk from it is the
   type-bound indirect walk (issue 2658's over-approximation applies).
2. **A stated stack per task.** The board's build states one number; a sched
   task's stack comes from the image's `[tasks.*]` table (and a per-platform
   default), so it is a per-image, per-task fact that the build, not the
   board, holds.
3. **A measured row** before it is enforced, as ruling (A) requires.

## Where it was found

Phase-557 W5b (2026-11-21): NuttX was made to state its init task's stack. The
phase doc's first draft named the sched-task stack for that, which would have
checked the wrong thread. The sched tasks were left out, as FreeRTOS's are, and
this issue records that every RTOS has the same gap.

## Fix direction

One role per sched task: root at the board's task trampoline for that task's
executor, stated from the image's resolved `[tasks.*]` sizes (the sizing
descriptor already carries them for ThreadX's pool, issue 2732), reported
unmeasured until a twin measures a peak.
