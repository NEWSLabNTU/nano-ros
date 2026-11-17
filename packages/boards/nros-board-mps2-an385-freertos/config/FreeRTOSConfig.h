/* FreeRTOS kernel configuration — QEMU MPS2-AN385 (Cortex-M3).
 * phase-337 W5.a: the board supplies the two board facts; the rest is shared. */
#define NROS_BOARD_CPU_CLOCK_HZ 25000000 /* QEMU MPS2-AN385 default */
#define NROS_BOARD_PRIO_BITS    3        /* 3 NVIC priority bits = 8 levels */

/* Issue 2507 -- every task stack carries an MPU guard over its lowest bytes,
 * reprogrammed on each context switch (`c/board_mps2.c`). Without it an
 * overflow writes over the heap_4 block below the stack, and the image reports
 * `*** MALLOC FAILED ***` instead of the task that overflowed. The knob
 * NROS_FREERTOS_TASK_STACK_GUARD=0 (either road) defines this to 0. */
#ifndef NROS_BOARD_TASK_STACK_GUARD
#define NROS_BOARD_TASK_STACK_GUARD 1
#endif

#include "../../nros-board-freertos/config/FreeRTOSConfig.h"
