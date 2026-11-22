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

## Progress (2026-11-22): C entries on FreeRTOS and Zephyr

Landed:

- **One role per sched task.** `TASK_<NAME>`, from the entry's
  `nros_priority_plan.json`, which `codegen entry` writes beside the TU
  (`--sched-tasks`).
- **Its root is the board's runner thread entry** (`--task-root`; `?sym` marks
  one an image may not link):
  - FreeRTOS: `freertos_level_task`, `freertos_sched_task`.
  - Zephyr: `zephyr_sched_task`.

  All tasks share it, so it is walked once.
- **Its stack is the static the C entry reserves for it**
  (`__nros_sched_task_stack_<i>`, `<rclc/detail/sched_task_memory_*.h>`), read
  off the linked image. The entry's macro has already applied the stated
  default and the port's floor, so no rule is restated. A task the entry gives
  no memory runs on the boot thread, which OPEN checks. Without reserved
  memory the stack is the task's authored `stack_bytes`, else the board's
  stated default (a `TASK` row on the stated channel). A task none of these
  covers is reported as not checked, never passed.
- **Reaches the build:**
  - FreeRTOS: `nros_stack_floor_freertos_check(... SCHED_TASKS ...)`.
  - Zephyr: the check script is written at the end of the configure, after
    the application's entry recorded its plan.

Measured on realtime-c `[image.freertos]`:

- `TASK_HIGH` 262,144 B (its reserved stack) holds 210,120 B.
- `TASK_LOW` runs on the boot thread.
- Every task floor is an upper bound today: the deepest path crosses
  `_z_network_message_elem_copy`, bound by type. Its warning says so and gives
  no size.

Open:

- **Rust entries** (`nros::main!`): no plan file, so no task roles. Their
  trampoline is `sched_task_entry<B, F, E>`, and their default stack is the
  board's, which the `TASK` stated row exists for.
- **NuttX:** its runner (`nuttx_level_thread`, `nuttx_sched_task_thread`) is
  reached through the cargo road's stated rows. Not wired.
- **ThreadX:** after issue 2732.
- **Rooting each task at its own callbacks, after issue 2658.** It is feasible:
  each `__nros_entry_setup_sched_task_<i>` loads only its task's callbacks.
  But the floor is dominated by type-bound internal calls every task shares.

## Progress (2026-11-22): Rust entries, and NuttX

- **One rule for a Rust entry's reserved stacks.** It is
  `BoardFamily::rust_entry_task_stacks` in `nros-entry-lower`: `nros::main!`
  reserves exactly these (`__NROS_SCHED_TASK_MEMORY_<i>`), and `nros build`
  writes them, with each task's authored `stack_bytes` and the family's boot
  task, to the generated entry's `nros-sched-tasks.json`.
- **The cargo road's walk takes that file.** The board states a `TASK` row
  (root + default) on the stated channel:
  - FreeRTOS: `~sched_task_entry`, `app_stack_bytes`.
  - NuttX: `~nuttx_level_trampoline`, `NUTTX_SCHED_TASK_STACK_DEFAULT_BYTES`.
    That value now lives in `nros-board-common`, which both the runner and the
    statement read.
- **The Zephyr check reads the file of a Rust west application.** Its default
  is `CONFIG_NROS_ZEPHYR_SCHED_TASK_STACK_SIZE`, which the configure reads and
  the cargo lane cannot.
- **When the image links no reservation** (a `board = X` image, which
  `nros::main!` gives no task memory), every task takes the board's default.

Measured, realtime-rust:

- `[image.freertos_realtime]`: TASK_HIGH holds 162,824 B on its reserved
  262,144 B. TASK_LOW is the boot task.
- `[image.nuttx]`: TASK_HIGH's 65,536 B is below its 96,208 B UPPER BOUND, a
  warning with no size given. TASK_LOW is the boot task.

No in-tree ARM Zephyr Rust image has sched tasks, so the Zephyr Rust wiring is
covered by the walker's self-test only. Left open: ThreadX (after issue 2732),
the C runners' own fallback stacks where no entry memory is reserved, and
rooting each task at its own callbacks (after issue 2658).

## Progress (2026-11-23): ThreadX, and derived level stacks

- **The file states the stacks the entry allocates.** `nros-sched-tasks.json`
  carried each task's AUTHORED `stack_bytes`. A level that authors none takes
  its largest member's `[[component]] stack_bytes` plus the worker frame in
  `nros::main!` (`derive_level_stacks`), and the file read that level as the
  port's default. `nros build` now derives the table with the macro's own
  function (`nros_orchestration_ir::derive_level_stacks_for`, of which
  `derive_level_stacks` is the Rust-runner case) before writing it.
- **The boot task is the Rust runner's rule** (`BoardFamily::rust_boot_sched_task`),
  which adds ThreadX: its runner boots `sched_tasks[0]` on the app thread and
  reserves no task memory (the byte pool holds the stacks).
- **rv-virt-threadx states a `TASK` row**: root `~level_task_entry`
  (`nros_board_threadx`'s spawned-level entry), default `app_stack_bytes`,
  which `nros_threadx_create_task` gives a task that states no stack.
  threadx-linux stays reported, not checked (a host image's frames run through
  the host libc; issue 2834).

Measured, realtime-rust with a temporary `[image] board = "rv-virt-threadx"`
(cargo road): OPEN floor 206,680 B and TASK_HIGH floor 203,880 B, each on a
524,288 B stack, both reported (no observed peak yet). TASK_LOW is the boot
task.

Left open: the C runners' own fallback stacks where no entry memory is
reserved, and rooting each task at its own callbacks (after issue 2658).
