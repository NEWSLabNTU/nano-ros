---
id: 2507
title: "FreeRTOS mps2-an385: a task stack overflow corrupts the heap and reads as MALLOC FAILED"
status: open
type: bug
area: boards, memory, freertos
related: [issue-2494, issue-2400, phase-553]
found: 2026-11-13
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
