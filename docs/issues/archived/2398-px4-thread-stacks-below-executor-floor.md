---
id: 2398
title: "On NuttX, both threads a generated PX4 entry runs nano-ros on start below the executor's stack floor: the main task gets PX4's 2048 B STACK_MAIN, the `wq:rate_ctrl` work queue 3150 B, the floor is 3808 B, and the measured need is higher still"
status: resolved
type: bug
severity: high
area: [px4, nuttx, memory, build]
related: [phase-542, phase-547, 0667, 0961, 1146]
found: 2026-11-10
---

**Home: [phase-542](../../roadmap/phase-542-px4-supported-platform.md)**, W8.b
(PX4 on a flight controller's NuttX). Found during phase-547 W3, the
emulated Pixhawk 6X spike, while issue 2397 was being measured.

## What

A generated PX4 entry (RFC-0113 D9) runs nano-ros on two kinds of thread, and
nano-ros sizes neither:

- **The module's main task.** `<main> start` runs `task_spawn`, which opens the
  session and every sched task's executor before it returns. On NuttX that
  task is a builtin whose stack is the module's `STACK_MAIN`. PX4's
  `px4_add_module` defaults it to 2048 B, and `platforms/nuttx/NuttX/CMakeLists.txt`
  writes it into the builtin table. The firmware example passes none.
- **One PX4 work queue per sched task.** The most urgent sched task gets
  `wq:rate_ctrl` (`nros_orchestration_ir::px4::px4_work_queues`). Its thread
  runs every executor pass. Its stack is the board option
  `CONFIG_WQ_RATE_CTRL_STACKSIZE`, 3150 B by default (Kconfig range
  1000..10000). The other three queues default lower or close:
  `hp_default` 2800, `nav_and_controllers` 2240, `lp_default` 3500.

The executor's stack floor for the fmu-v6x cross-build
(`thumbv7em-nuttx-eabihf`, `nros-cpp`) is `NROS_EXECUTOR_MAIN_STACK_MIN` =
**3808 B**. Both stock threads are below it. Nothing checked, so a stock
configuration built and then overflowed at run time. On NuttX a task's stack
is allocated from the heap, so the overflow writes into the heap block below
it and surfaces later, in someone else's `free`.

## Where 3808 comes from

`nros-build-helpers/src/cpp.rs` (and the C templates): `executor_stack_min =
2 x EXECUTOR_VALUE_SIZE`, where `EXECUTOR_VALUE_SIZE` is the sizes probe's
`size_of` of the executor value for the target, 1904 B on thumbv7em. It is a
computed bound, not a measured peak: `Executor::open_in` builds the value in
its frame and `nros_cpp_init` holds it again, so one value is on the stack
twice. The header itself calls it necessary, not sufficient (issue 0961).

## Measured on Cortex-M7

Method: static, from the compiler's own frame sizes. `nros-cpp` was
cross-built exactly as the phase-547 spike built it (features
`alloc,log,panic-platform,rmw-cffi,platform-nuttx`, `-Zbuild-std`,
`NUTTX_DIR` = PX4 1.17's NuttX tree, Arm GNU 13.2.1 for the C parts) with
`RUSTFLAGS=-Z emit-stack-sizes`, once at the release profile's `opt-level=3`
and once at `opt-level=z` (what the spike shipped to fit flash). Each
function's frame is read from the archive's `.stack_sizes` sections with
`llvm-readobj --stack-sizes`. The emulator was not available, so there is no
run-time high-water mark yet.

Frames on the two paths (bytes, O3 / Oz):

| thread | function | O3 | Oz |
|---|---|---|---|
| main task | `nros_cpp_init` | 56 | 24 |
| | `nros_cpp::init_rmw_over` | 2128 | 2128 |
| | `Executor::open_in` | 2840 | 2872 |
| | `CffiRmw::open_with_rmw` | 648 | 600 |
| | `CffiSession::open_marshalling_properties` | 8472 | 8464 |
| | `CffiSession::open_with_vtable` | 1168 | 1192 |
| | **sum, before any backend frame** | **15312** | **15280** |
| work queue | `nros_cpp_spin_once` | 64 | 64 |
| | `Executor::spin_once_shared` | 48 | 48 |
| | `Executor::pass` | 2720 | 4064 |
| | **sum, before any callee of `pass`** | **2832** | **4176** |

At O3, following direct calls (`bl` relocations) from `nros_cpp_spin_once`
gives a deepest path of 3624 B, with 70 indirect calls on it that a static
walk cannot follow (the backend's vtable, the C++ callbacks). From
`nros_cpp_init` it gives 15788 B.

So, against the stock sizes:

- The main task needs at least 15.3 KB, about **7.5x** PX4's 2048 B default
  and about **4x** the 3808 B floor.
- One pass needs at least 3624 B at O3 and 4176 B at Oz, both above
  `wq:rate_ctrl`'s 3150 B, before the uORB backend or any callback adds its
  frames.

## Decision: refuse at configure (A), and do not lower the floor (B)

**(B) is wrong by measurement.** 3808 is not conservative. The open path is
four times it, and one pass is already at or above it with only the
executor's own frames. Lowering it would let more broken configurations
through.

**(A), refusing, for both threads.** CLAUDE.md's rule (issue 0667) is that a
stack request is a floor the port raises. Neither thread can be raised here:

- A work queue's stack is the board's Kconfig `int`, owned by PX4's
  `px4_work_queue`, and the queue is shared with PX4's own modules. A module
  cannot change it. (A module could create its own `wq_config_t` with its own
  stack, but RFC-0113 D9 deliberately maps sched tasks onto PX4's queues, so
  that would be a design change, not a fix.)
- `STACK_MAIN` is the module's own argument, so the port could raise an unset
  one to the floor. It does not, because the floor is measured to be about a
  quarter of what opening the session takes. Raising to it would turn a
  configure-time refusal into a configure that passes and an image that
  overflows its main task.

So `nros_px4_add_module` now refuses, by knob name, at configure time.

## Fix

- **`integrations/px4/NanoRosPx4ThreadStacks.cmake`** (new):
  - `nros_px4_read_stack_floor` reads `NROS_EXECUTOR_MAIN_STACK_MIN` from the
    per-build C++ config header the module compiles against. That is the
    header the archive's own cargo run placed (issue 1824), so the floor is
    the archive's own target's.
  - `nros_px4_entry_work_queues` reads each sched task's queue from the
    generated entry's `SchedTaskWorkItem("<item>", px4::wq_configurations::<q>)`
    calls.
  - `nros_px4_check_thread_stacks` compares each queue's effective stack, and
    on NuttX the main task's `STACK_MAIN`, against the floor. It collects every
    shortfall and refuses once. A missing `CONFIG_WQ_<Q>_STACKSIZE`, an unset
    `STACK_MAIN` on NuttX, and a `STACK_MAIN` that is not a byte count are
    refused too.
  - The effective stack is PX4's own arithmetic (`WorkQueueManager.cpp`,
    `PX4_STACK_ADJUSTED`): the configured size times `sizeof(void *) / 4`,
    plus 24 KiB on POSIX. So SITL (`3150 * 2 + 24576`) is unaffected, and
    `STACK_MAIN` is checked on NuttX only, the one platform where PX4 sizes a
    builtin's task from it.
- **`integrations/px4/NanoRosPx4Module.cmake`** calls the three functions for
  every generated entry (`BRINGUP`), before `px4_add_module`.
- **Gate `just check px4-thread-stacks`** (`scripts/check-px4-thread-stacks.sh`,
  fast lane): 14 `cmake -P` cases over the real functions. The stock NuttX
  queue and main task are refused. Raised values and stock SITL are accepted.
  A header without the floor is refused. The queues are read from the
  codegen's own golden output (`px4_sched_tasks.cpp.golden`). The module's
  wiring is checked too.

## Evidence

- **Gate:** 19 ok lines. Mutations, each one red: the queue comparison
  disabled; PX4's POSIX overhead dropped; the 64-bit doubling dropped; qurt
  treated as POSIX; the main-task check never run; an unset `STACK_MAIN`
  accepted; the floor read from the wrong define; the module's `include`
  removed. One mutation found a real bug while the gate was being written:
  `if(T_STACK_MAIN STREQUAL "")` on an empty argument tested the variable's
  NAME, so an unset `STACK_MAIN` was reported as "not a byte count". It now
  quotes the value.
- **A real PX4 configure** (PX4 1.17.0 at our pin, `px4_fmu-v6x_default`, the
  firmware example as `EXTERNAL_MODULES_LOCATION`, the thumbv7em archive and
  its placed headers):
  - stock board, no `STACK_MAIN`: refused, naming both
    `CONFIG_WQ_RATE_CTRL_STACKSIZE=3150` (for sched task
    `nros_uorb_demo_default` on `wq:rate_ctrl`) and the 2048 B main task.
  - `STACK_MAIN 16384`, stock board: refused, naming only the queue.
  - `STACK_MAIN 16384`, and a board label adding
    `CONFIG_WQ_RATE_CTRL_STACKSIZE=8192` (merged over `default.px4board`, the
    way PX4 merges a label): configure passes, and the builtin table reads
    `{ "nros_uorb_demo", SCHED_PRIORITY_DEFAULT, 16384 * (__SIZEOF_POINTER__ >> 2), ... }`.
  - SITL (`px4_sitl_default`, x86_64 host archive, stock queues, no
    `STACK_MAIN`): configure passes, as before. The host floor is 4864 B;
    `wq:rate_ctrl`'s thread is 3150 * 2 + 24576 = 30876 B there.

## What the floor does not do

The check makes a thread below the floor impossible to configure. It does
not make a thread at the floor safe: the measurement above says the open
path needs about 4x the floor, and a pass sits at the floor before its
callees. So the refusal names the floor as the minimum and points here for
sizing. Until a run-time high-water mark exists, size `STACK_MAIN` from the
open path's 15.3 KB plus the backend's open, and the queue from the pass's
3.6 to 4.2 KB plus the callbacks', inside PX4's 10,000 B cap.

## Open

- **A measured floor.** The derived floor (2x the executor value) describes
  `open_in`'s frame, not the open path or a pass. A sizes-probe quantity
  derived from the build's own frames (`-Z emit-stack-sizes` is nightly-only,
  and the NuttX cross-build already uses nightly) would make the refusal
  sufficient instead of only necessary. It is shared stack-floor code, not
  PX4 integration code, so it is not done here.
- **A run-time peak.** Issue 1146's practice is that every image prints its
  own stack peak at boot. The PX4 entry does not yet; NuttX's
  `CONFIG_STACK_COLORATION` makes it available once the emulator or a board
  runs the module.
- **`CffiSession::open_marshalling_properties` has an 8.4 KB frame,** the
  largest single frame on either path. It is reached from every backend's
  open. Shrinking it lowers every RTOS's main-task need, not only PX4's.
- **A lead for issue 2397.** That issue's fault is a MemManage in `mm_free`
  by `netinit`, just after `nros: session open (rmw=uorb)`, with `wq:rate_ctrl`
  already raised to 24,000 B. The spike's module passed no `STACK_MAIN`, so
  its main task had 2048 B against a measured open path of more than 15 KB,
  and a NuttX task stack is a heap block. An overflow that deep writes over
  the heap below the task's stack, and the next `free` of a neighbouring
  block faults. That matches the symptom; it is not yet confirmed by a run.
