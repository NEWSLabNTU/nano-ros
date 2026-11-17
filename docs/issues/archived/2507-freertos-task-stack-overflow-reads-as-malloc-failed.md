---
id: 2507
title: "FreeRTOS mps2-an385: a task stack overflow corrupts the heap and reads as MALLOC FAILED"
status: resolved
type: bug
area: boards, memory, freertos
related: [issue-2494, issue-2400, phase-553]
found: 2026-11-13
resolved: 2026-11-17
resolved_in: "candidate fix 1: an MPU guard over each task stack's lowest 256 bytes, moved on every context switch, with a HardFault report naming the task"
---

## What is wrong

On the FreeRTOS mps2-an385 board, a task stack overflow is not caught where it happens and is reported as the wrong failure.

- Task stacks are heap_4 blocks: `xTaskCreate` allocates each stack with `pvPortMalloc`. Another heap block lies directly below every task stack.
- An overflow writes over the next block's header first. The following `pvPortMalloc` or `vPortFree` then fails, so the image prints `*** MALLOC FAILED ***` and hangs.
- `configCHECK_FOR_STACK_OVERFLOW 2` is set and `vApplicationStackOverflowHook` is wired up. But the kernel checks the stack's fill pattern only at a context switch, and the heap usually fails first. `freertos_config.rs` (`app_stack_bytes`) records the measured case: `NROS_FREERTOS_APP_STACK_KB=32` gives `MALLOC FAILED`, not `STACK OVERFLOW`.

So an undersized task stack corrupts heap metadata. The report then points at the allocator, not at the stack.

## Not covered by the bare-metal fix

phase-553 W7 (issue 2494) guards the bare-metal board's MAIN stack with one MPU region below it. That does not carry over to FreeRTOS: there are many task stacks, and each lives at a heap address chosen at run time.

## Candidate fixes

1. **An MPU region per task, reprogrammed on every context switch.** Put a no-access region over the lowest bytes of the incoming task's stack, using `traceTASK_SWITCHED_IN` or FreeRTOS-MPU's `portSTORE_TASK_MPU_SETTINGS`. Region size and alignment must be a power of two, so the guard either sits inside the stack, aligned up from `pxTaskGetStackStart`, or the stacks are allocated aligned. The cost is a few register writes per switch. The fault arrives on the overflowing store, so the report names the right task.
2. **A canary or guard gap between heap blocks.** Pad each stack allocation with a painted gap and check it at a switch or on `pvPortMalloc`. That is cheaper and works without an MPU. But it detects after the fact, and a write that skips the gap still lands in the next block.

## Constraint

QEMU's mps2-an385 does not fault on a write below the SRAM base (0x2000_0000; measured on QEMU 6.2 and 11.0). So a guard on this machine must be an MPU region, not a layout that relies on running off the end of RAM. The MPU does fault on both QEMU versions (measured in phase-553 W7).

## Fixed (2026-11-17)

Candidate fix 1, an MPU region per task moved on every context switch:

- `portTASK_SWITCH_HOOK`, the kernel's own hook for reprogramming the MPU
  (separate from the trace hooks `NROS_TRACE` defines), calls
  `nros_board_task_stack_guard_arm` with the incoming task's `pxStack`. It
  moves MPU region 0 over that stack's lowest 256 bytes, aligned up. Nothing
  else on the board programs the MPU (the GCC ARM_CM3 port is the non-MPU
  port), and `PRIVDEFENA` keeps the default map everywhere else.
- The region is privileged READ-ONLY, not no-access. The kernel's
  `configCHECK_FOR_STACK_OVERFLOW 2` pattern check and
  `uxTaskGetStackHighWaterMark` read those bytes; an overflow writes them.
- MemManage stays disabled, so the fault escalates to HardFault. With
  `HFNMIENA` clear the MPU is off in the handler, which runs on the main stack.
  It matches `MMFAR` (or `MSTKERR` and PSP) against the guard, prints
  `*** STACK OVERFLOW: <task> *** (the task's stack hit its guard)` and exits
  QEMU with a failure status. Any other HardFault prints `*** HARD FAULT ***`;
  before this, the vector was an infinite loop.
- Knob `NROS_FREERTOS_TASK_STACK_GUARD=0` (registry; the cargo road's reader is
  `nros-board-freertos/build.rs`, the cmake road's
  `cmake/platform/nano-ros-freertos.cmake`) builds a kernel that never arms it.
  The default is on, on both roads and in every profile.

Limits: the first task runs unguarded until the first switch, and a single
frame larger than 256 bytes can store below the guard without touching it.

Measured on QEMU 11.0 with `bins/stack-guard-freertos-mps2` (test
`freertos_stack_guard`):

| | guard off | guard on |
| --- | --- | --- |
| app task high-water | 130,680 B | 130,680 B |
| a 2 KiB task recursing without bound | the kernel's check caught it on some runs; the others hung silently | `STACK OVERFLOW: overflow`, every run |
| 400,000 switches, `-icount shift=0` (instruction count) | 34 ticks | 42 ticks (about 20 instructions a switch) |
| 400,000 switches, `-icount shift=auto` (host time) | 454 ticks | 1,800 ticks (QEMU flushes its TLB on every MPU write) |

Every FreeRTOS mps2 cell in `just freertos test` (`rtos_e2e` action, service,
pubsub and quiet-link in Rust, C and C++, and `realtime_sched_tasks`' three
FreeRTOS rows) gave the same verdict in the same time within noise, two runs
with the guard and two without. No cell sets the knob.

