/*
 * freertos_task_glue.c — FreeRTOS kernel helpers with no network dependency.
 *
 * phase-370 W1. These three functions were part of `network_glue.c` until the
 * FreeRTOS POSIX simulator board arrived. They touch no lwIP symbol, but that
 * TU includes `lwip/init.h`, `lwip/tcpip.h`, `lwip/netif.h`, `lwip/ip4_addr.h`
 * and `lwip/sockets.h` unconditionally — reasonably, since its whole subject is
 * lwIP bring-up. A board whose network stack is the HOST kernel's has no lwIP
 * to include, and needed exactly these three.
 *
 * Split rather than copied: a second `xTaskCreate` wrapper in a second file is
 * how the stack-depth truncation this one documents would have come back in
 * only one of them. Every FreeRTOS board compiles this TU; only boards with
 * lwIP also compile `network_glue.c`.
 */

#include <stdint.h>

#include "FreeRTOS.h"
#include "task.h"

/*
 * Start the FreeRTOS scheduler.  Does not return.
 */
void nros_freertos_start_scheduler(void) {
    vTaskStartScheduler();
    /* Should never reach here */
    for (;;) {}
}

/*
 * Create a FreeRTOS task.
 * Returns 0 on success, -1 on failure.
 */
int nros_freertos_create_task(
    void (*entry)(void *),
    const char *name,
    uint32_t stack_words,
    void *arg,
    uint32_t priority)
{
    /* configSTACK_DEPTH_TYPE defaults to StackType_t (uint32_t on Cortex-M3
     * via portmacro.h). The previous (uint16_t) cast silently truncated
     * stack depths > 65535 words (>256 KB), leaving tasks with a 0-word
     * stack and a wild SP. Drop the cast — xTaskCreate accepts the full
     * uint32_t we already declared in this wrapper. */
    BaseType_t ret = xTaskCreate(entry, name, stack_words, arg,
                                 (UBaseType_t)priority, NULL);
    return (ret == pdPASS) ? 0 : -1;
}

/*
 * Set the CALLING task's priority (raw FreeRTOS units), clamped to
 * configMAX_PRIORITIES - 1 (the shared FreeRTOSConfig.h defines a live
 * configASSERT, so an out-of-range value must not reach
 * vTaskPrioritySet). Used by the multi-tier boot path: the boot task is
 * created at the generic app priority but then RUNS tiers[0] (the
 * highest tier), so it must assume that tier's declared priority the
 * same way a spawned tier task is born with its own.
 */
void nros_freertos_set_current_task_priority(uint32_t priority)
{
    if (priority >= (uint32_t)configMAX_PRIORITIES) {
        priority = (uint32_t)configMAX_PRIORITIES - 1u;
    }
    vTaskPrioritySet(NULL, (UBaseType_t)priority);
}
