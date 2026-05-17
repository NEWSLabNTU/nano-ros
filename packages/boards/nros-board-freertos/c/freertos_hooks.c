/*
 * freertos_hooks.c — generic FreeRTOS kernel hooks
 *
 * Phase 149.1.B.1 — extracted from build.rs's `STARTUP_C` const
 * as part of the per-board → generic-crate refactor. Contains
 * hook functions FreeRTOS calls back into (assert, idle, malloc-
 * failed, stack-overflow, SysTick) plus the semihosting helpers
 * those hooks use to report failures. No board-specific code
 * here — `freertos_hooks.c` is the candidate for promotion into
 * the generic `nros-board-freertos` crate in 149.1.B.4.
 */

#include <stdint.h>

#include "FreeRTOS.h"
#include "task.h"

/* ---- Semihosting helpers ---- */

void semihosting_write0(const char *s) {
    __asm__ volatile("mov r0, #0x04\n"
                     "mov r1, %0\n"
                     "bkpt #0xAB\n"
                     :
                     : "r"(s)
                     : "r0", "r1", "memory");
}

static void semihosting_write_int(int val) {
    char buf[12];
    char *p = buf + sizeof(buf) - 1;
    *p = '\0';
    if (val < 0) { semihosting_write0("-"); val = -val; }
    if (val == 0) { semihosting_write0("0"); return; }
    while (val > 0) { *--p = '0' + (val % 10); val /= 10; }
    semihosting_write0(p);
}

/* Phase 121.3.freertos-parity — alias used by hook callers. */
static void semihost_write0(const char *s) {
    semihosting_write0(s);
}

/* ---- FreeRTOS assert ---- */
void freertos_assert_failed(const char *file, int line) {
    semihosting_write0("FreeRTOS ASSERT FAILED: ");
    semihosting_write0(file);
    semihosting_write0(":");
    semihosting_write_int(line);
    semihosting_write0("\n");
    __asm__ volatile("bkpt #0");
    for (;;) {}
}

/* ---- FreeRTOS malloc failed hook ---- */
void vApplicationMallocFailedHook(void) {
    semihost_write0("*** MALLOC FAILED ***\n");
    for (;;) { __asm__ volatile("wfi"); }
}

/* Phase 121.3.freertos-parity diagnostic — stack overflow hook.
 * Prints offending task name via semihosting so the regression
 * is localized at runtime instead of silently hanging. */
void vApplicationStackOverflowHook(TaskHandle_t xTask, char *pcTaskName) {
    (void) xTask;
    semihost_write0("*** STACK OVERFLOW: ");
    semihost_write0(pcTaskName);
    semihost_write0(" ***\n");
    for (;;) { __asm__ volatile("wfi"); }
}

/* ---- FreeRTOS idle hook: WFI for QEMU ---- */
/* On real hardware, WFI saves power. In QEMU, it yields CPU time back to
 * the main event loop so that the TAP network FD can be serviced. Without
 * this, the idle task busy-loops and QEMU never processes incoming network
 * frames from the host (ARP replies, TCP SYN-ACKs, etc.). */
void vApplicationIdleHook(void) {
    __asm__ volatile("wfi");
}

/* ---- FreeRTOS SysTick handler ---- */
extern void xPortSysTickHandler(void);

void SysTick_Handler(void) {
    if (xTaskGetSchedulerState() != taskSCHEDULER_NOT_STARTED) {
        xPortSysTickHandler();
    }
}
