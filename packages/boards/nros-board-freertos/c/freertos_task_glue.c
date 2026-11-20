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

#include <stddef.h>
#include <stdint.h>

#include "FreeRTOS.h"
#include "task.h"

#include "nros_freertos_task_create.h" /* issue 2632 */

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
    BaseType_t ret = NROS_FREERTOS_TASK_CREATE(entry, name, stack_words, arg,
                                 (UBaseType_t)priority, NULL);
    return (ret == pdPASS) ? 0 : -1;
}

/*
 * Issue 1598 — create a FreeRTOS task over CALLER-OWNED memory: the Rust
 * `nros::main!` entry's `SchedTaskMemory` static (stack + control-block region),
 * so a sched task's stack is `.bss` the linker prices instead of a heap_4 block.
 *
 * `tcb_bytes` is the region the entry reserved for the control block, an upper
 * bound stated in Rust (`SCHED_TASK_TCB_U64S`) because Rust cannot name a C
 * type's size. It is CHECKED here, where `sizeof(StaticTask_t)` is known: a
 * kernel whose control block outgrew the bound is refused by name rather than
 * overrunning. Returns 0 on success, -1 on refusal or failure.
 */
int nros_freertos_create_task_static(void (*entry)(void*), const char* name, void* stack,
                                     uint32_t stack_bytes, void* arg, uint32_t priority, void* tcb,
                                     uint32_t tcb_bytes) {
#if (configSUPPORT_STATIC_ALLOCATION == 1)
    if (stack == NULL || tcb == NULL || tcb_bytes < (uint32_t)sizeof(StaticTask_t) ||
        stack_bytes < (uint32_t)(configMINIMAL_STACK_SIZE * sizeof(StackType_t))) {
        return -1;
    }
    TaskHandle_t h = NROS_FREERTOS_TASK_CREATE_STATIC(entry, name, stack_bytes / (uint32_t)sizeof(StackType_t),
                                       arg, (UBaseType_t)priority, (StackType_t*)stack,
                                       (StaticTask_t*)tcb);
    return (h != NULL) ? 0 : -1;
#else
    (void)entry;
    (void)name;
    (void)stack;
    (void)stack_bytes;
    (void)arg;
    (void)priority;
    (void)tcb;
    (void)tcb_bytes;
    return -1;
#endif
}

/*
 * Set the CALLING task's priority (raw FreeRTOS units), clamped to
 * configMAX_PRIORITIES - 1 (the shared FreeRTOSConfig.h defines a live
 * configASSERT, so an out-of-range value must not reach
 * vTaskPrioritySet). Used by the multi-sched-task boot path: the boot task is
 * created at the generic app priority but then RUNS sched_tasks[0] (the
 * highest sched task), so it must assume that sched task's declared priority the
 * same way a spawned sched task is born with its own.
 */
void nros_freertos_set_current_task_priority(uint32_t priority)
{
    if (priority >= (uint32_t)configMAX_PRIORITIES) {
        priority = (uint32_t)configMAX_PRIORITIES - 1u;
    }
    vTaskPrioritySet(NULL, (UBaseType_t)priority);
}

/*
 * Issue 2016 -- the executor lock's thread identity for the Rust level
 * runner (`run_sched_tasks_entry`): the running task's handle, as the C runner's
 * `freertos_thread_id` gives it (`freertos_run_sched_tasks.c`).
 */
uintptr_t nros_freertos_current_task_id(void)
{
    return (uintptr_t)xTaskGetCurrentTaskHandle();
}

/*
 * The task census: every task the kernel holds, with its priority, printed
 * once when the image is fully up (after the last level's setup). A priority
 * plan reserves bands for threads nano-ros does not create (lwIP's
 * `tcpip_thread`, the kernel's timer service task); FreeRTOS has no procfs to
 * read them from, so the image says what is there and a test classifies each
 * task against the plan. One line per task:
 *
 *   nros: task census name=`<name>` prio=<n>
 *
 * then `nros: task census done tasks=<n>`. Without the kernel's trace
 * facility it says so instead, and the plan's foreign band stays unmeasured.
 */
extern void nros_board_freertos_console_write(const char* s);
extern void* nros_platform_alloc(size_t size);
extern void nros_platform_dealloc(void* ptr);

static const char* census_dec(uint32_t v, char* buf, size_t len) {
    size_t i = len - 1u;
    buf[i] = '\0';
    do {
        buf[--i] = (char)('0' + (v % 10u));
        v /= 10u;
    } while (v != 0u && i > 0u);
    return &buf[i];
}

void nros_board_freertos_report_task_census(void) {
#if defined(configUSE_TRACE_FACILITY) && configUSE_TRACE_FACILITY
    char nb[12];
    /* Room for tasks created between the count and the read. */
    UBaseType_t cap = uxTaskGetNumberOfTasks() + 4u;
    TaskStatus_t* st = (TaskStatus_t*)nros_platform_alloc(cap * sizeof(TaskStatus_t));
    if (st == NULL) {
        nros_board_freertos_console_write("nros: task census unavailable: no heap for it\n");
        return;
    }
    UBaseType_t n = uxTaskGetSystemState(st, cap, NULL);
    for (UBaseType_t i = 0; i < n; ++i) {
        nros_board_freertos_console_write("nros: task census name=`");
        nros_board_freertos_console_write(st[i].pcTaskName != NULL ? st[i].pcTaskName : "?");
        nros_board_freertos_console_write("` prio=");
        nros_board_freertos_console_write(
            census_dec((uint32_t)st[i].uxBasePriority, nb, sizeof(nb)));
        nros_board_freertos_console_write("\n");
    }
    nros_board_freertos_console_write("nros: task census done tasks=");
    nros_board_freertos_console_write(census_dec((uint32_t)n, nb, sizeof(nb)));
    nros_board_freertos_console_write("\n");
    nros_platform_dealloc(st);
#else
    nros_board_freertos_console_write(
        "nros: task census unavailable: configUSE_TRACE_FACILITY is 0\n");
#endif
}

/* The runner-neutral name the single-executor runner calls
 * (`nros_board_optional_hook.h`). */
void nros_board_report_task_census(void) {
    nros_board_freertos_report_task_census();
}
