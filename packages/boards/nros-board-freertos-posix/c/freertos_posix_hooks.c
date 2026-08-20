/*
 * freertos_posix_hooks.c — FreeRTOS kernel hooks for the POSIX simulator.
 *
 * phase-370 W1. The family's `nros-board-freertos/c/freertos_hooks.c` cannot be
 * reused here: every one of its bodies is ARM inline assembly — semihosting
 * `bkpt #0xAB` to report, `wfi` to idle, and a `SysTick_Handler` for a core
 * that does not exist in a host process. This file answers the same five
 * questions for a host, and nothing in it is a copy of that one.
 *
 * The two static-allocation hooks have no family counterpart at all: the
 * Cortex-M config sets `configSUPPORT_STATIC_ALLOCATION 0`, while the POSIX
 * port needs it on (see `config/FreeRTOSConfig.h`), which makes supplying the
 * idle and timer task memory the application's job.
 */

#include <stdio.h>
#include <stdlib.h>

#include "FreeRTOS.h"
#include "task.h"

/* ---- Static allocation ----
 *
 * With `configSUPPORT_STATIC_ALLOCATION 1` the kernel asks the application for
 * the memory backing its two internal tasks rather than allocating it. Both
 * hooks are mandatory in that configuration; omitting either is a link error
 * naming a symbol the application never wrote, which is why they are here
 * rather than in an example. */

static StaticTask_t idle_task_tcb;
static StackType_t idle_task_stack[configMINIMAL_STACK_SIZE];

static StaticTask_t timer_task_tcb;
static StackType_t timer_task_stack[configTIMER_TASK_STACK_DEPTH];

void vApplicationGetIdleTaskMemory(StaticTask_t **ppxIdleTaskTCBBuffer,
                                   StackType_t **ppxIdleTaskStackBuffer,
                                   configSTACK_DEPTH_TYPE *pulIdleTaskStackSize) {
    *ppxIdleTaskTCBBuffer = &idle_task_tcb;
    *ppxIdleTaskStackBuffer = idle_task_stack;
    *pulIdleTaskStackSize = configMINIMAL_STACK_SIZE;
}

void vApplicationGetTimerTaskMemory(StaticTask_t **ppxTimerTaskTCBBuffer,
                                    StackType_t **ppxTimerTaskStackBuffer,
                                    configSTACK_DEPTH_TYPE *pulTimerTaskStackSize) {
    *ppxTimerTaskTCBBuffer = &timer_task_tcb;
    *ppxTimerTaskStackBuffer = timer_task_stack;
    *pulTimerTaskStackSize = configTIMER_TASK_STACK_DEPTH;
}

/* ---- Family console hook ----
 *
 * phase-370 — `nros-board-freertos/c/freertos_run_tiers.c` reports the
 * placement-dim note through this. On the Cortex-M boards it forwards to ARM
 * semihosting; here it is `stderr`, for the same reason the entry TU's log
 * writer uses stderr: stdout carries the application's own output, which is
 * what an e2e test greps. */
void nros_board_freertos_console_write(const char *s) {
    if (s != NULL) {
        fputs(s, stderr);
        fflush(stderr);
    }
}

/* ---- Failure hooks ----
 *
 * All three report on stderr and then `abort()`. Deliberately NOT the family's
 * `for (;;) { wfi; }`: on a host, a spin loop turns a diagnosable crash into a
 * hung process that a test harness can only kill on timeout, and the timeout
 * message describes the harness rather than the fault. `abort()` raises SIGABRT
 * — a non-zero exit the lane reports, and a core file if the host keeps them. */

void freertos_assert_failed(const char *file, int line) {
    fprintf(stderr, "FreeRTOS ASSERT FAILED: %s:%d\n", file, line);
    fflush(stderr);
    abort();
}

void vApplicationMallocFailedHook(void) {
    /* heap_3 forwards to the host `malloc`, so reaching this means the HOST
     * allocator returned NULL, not that a `configTOTAL_HEAP_SIZE` budget ran
     * out. Say so: the number in FreeRTOSConfig.h is not the one that failed. */
    fprintf(stderr, "*** MALLOC FAILED (heap_3: the host allocator returned NULL) ***\n");
    fflush(stderr);
    abort();
}

void vApplicationStackOverflowHook(TaskHandle_t xTask, char *pcTaskName) {
    (void)xTask;
    /* Unreachable as configured — `configCHECK_FOR_STACK_OVERFLOW` is 0 because
     * the POSIX port's stacks belong to pthreads and the host guard page is the
     * real detector. Defined anyway so turning the check on for a debugging
     * session does not also require writing this. */
    fprintf(stderr, "*** STACK OVERFLOW: %s ***\n", pcTaskName ? pcTaskName : "?");
    fflush(stderr);
    abort();
}
