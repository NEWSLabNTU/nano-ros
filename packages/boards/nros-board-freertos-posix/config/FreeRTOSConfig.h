/*
 * FreeRTOS kernel configuration — POSIX simulator (phase-370 W1).
 *
 * This board owns the WHOLE file rather than the two-line shape the Cortex-M
 * boards use:
 *
 *     #define NROS_BOARD_CPU_CLOCK_HZ 25000000
 *     #define NROS_BOARD_PRIO_BITS    3
 *     #include "../../nros-board-freertos/config/FreeRTOSConfig.h"
 *
 * That is RFC-0064 ladder rung 3, taken deliberately. The shared family file's
 * two board facts are `configCPU_CLOCK_HZ` and `configPRIO_BITS`, and on the
 * POSIX port BOTH are meaningless: there is no NVIC to give priority bits to,
 * and the tick comes from a host timer signal rather than from a core clock —
 * the port's `port.c` reads neither. Supplying invented numbers so the include
 * would compile is how a config file starts lying about the hardware.
 *
 * The values below are the ones ASI's `freertos-posix` target has been running
 * its actuation module on, which is the consumer this board exists to migrate.
 *
 * Tuned for nros + CycloneDDS on the host:
 *   - Recursive mutexes (the RMW's own locking)
 *   - Static AND dynamic allocation: the POSIX port needs static for the idle
 *     and timer task memory (see `c/freertos_posix_hooks.c`), and dynamic for
 *     every task nano-ros creates through `nros_platform_task_create`.
 *   - Timer service (executor timers)
 */

#ifndef FREERTOS_CONFIG_H
#define FREERTOS_CONFIG_H

/* ---- Scheduler ----
 * No `configCPU_CLOCK_HZ`: the POSIX port drives the tick from a host interval
 * timer, and defining a core clock it never reads would be decoration. */
#define configUSE_PREEMPTION                    1
#define configUSE_PORT_OPTIMISED_TASK_SELECTION 0
#define configUSE_TICKLESS_IDLE                 0
#define configTICK_RATE_HZ                      ((TickType_t)1000)
#define configMAX_PRIORITIES                    10
/* In WORDS. 4096 words = 32 KB on a 64-bit host: the POSIX port allocates a
 * real pthread stack per task, and host libc frames (CycloneDDS, getaddrinfo)
 * are far larger than a Cortex-M's. The family's 256 would fault immediately. */
#define configMINIMAL_STACK_SIZE                ((unsigned short)4096)
#define configSTACK_DEPTH_TYPE                  uint32_t
#define configMAX_TASK_NAME_LEN                 32
#define configUSE_16_BIT_TICKS                  0
#define configIDLE_SHOULD_YIELD                 1
#define configTASK_NOTIFICATION_ARRAY_ENTRIES   3
#define configUSE_TASK_NOTIFICATIONS            1
#define configNUMBER_OF_CORES                   1

/* ---- Memory ----
 * heap_3 wraps the host `malloc`/`free`, so `configTOTAL_HEAP_SIZE` is not a
 * budget this port enforces — the host allocator is. It is still defined
 * because the kernel's static asserts reference it.
 *
 * Static allocation is ON, which the Cortex-M family config has OFF: the POSIX
 * port asks the application for the idle and timer task memory through
 * `vApplicationGetIdleTaskMemory` / `vApplicationGetTimerTaskMemory`. Those two
 * hooks are the reason, and they live in `c/freertos_posix_hooks.c`. */
#define configTOTAL_HEAP_SIZE                   ((size_t)(4 * 1024 * 1024))
#define configSUPPORT_STATIC_ALLOCATION         1
#define configSUPPORT_DYNAMIC_ALLOCATION        1
#define configAPPLICATION_ALLOCATED_HEAP        0

/* ---- Synchronisation ----
 * Recursive mutexes are not optional here: `nros_platform_mutex_rec_*` maps
 * onto them, and CycloneDDS's ddsrt takes a recursive lock on the participant. */
#define configUSE_MUTEXES                       1
#define configUSE_RECURSIVE_MUTEXES             1
#define configUSE_COUNTING_SEMAPHORES           1
#define configQUEUE_REGISTRY_SIZE               20
#define configUSE_QUEUE_SETS                    0

/* ---- Software timers ---- */
#define configUSE_TIMERS                        1
#define configTIMER_TASK_PRIORITY               (configMAX_PRIORITIES - 1)
#define configTIMER_QUEUE_LENGTH                20
#define configTIMER_TASK_STACK_DEPTH            (configMINIMAL_STACK_SIZE * 2)

/* ---- Hooks ----
 * No idle hook: the Cortex-M family config uses one to `wfi` so QEMU services
 * its network FD. Here the idle task is a pthread the host scheduler preempts
 * on its own, and a hook that spun would burn a core. */
#define configUSE_IDLE_HOOK                     0
#define configUSE_TICK_HOOK                     0
#define configUSE_DAEMON_TASK_STARTUP_HOOK      0
#define configUSE_MALLOC_FAILED_HOOK            1
/* Stack-overflow checking is OFF, and that is a property of the port rather
 * than a saving: FreeRTOS detects overflow by watermarking a stack it owns, and
 * on the POSIX port the stack belongs to pthreads. The host's guard page is the
 * real detector, and it reports a SIGSEGV with a usable core rather than a
 * hook. */
#define configCHECK_FOR_STACK_OVERFLOW          0

/* ---- Diagnostics ----
 * `configUSE_TRACE_FACILITY` is required by CycloneDDS's ddsrt: its
 * `ddsrt_gettid()` on FreeRTOS calls `vTaskGetInfo()`, which the kernel only
 * emits when this is on. Same reason the mps2 overlay turns it on from CMake;
 * here it is in the config file because there is no cross build to thread it
 * through. It does NOT enable nano-ros's optional tband trace hooks. */
#define configUSE_TRACE_FACILITY                1
#define configUSE_STATS_FORMATTING_FUNCTIONS    0
#define configGENERATE_RUN_TIME_STATS           0
#define configUSE_CO_ROUTINES                   0
#define configMAX_CO_ROUTINE_PRIORITIES         1

/* ---- API inclusion ---- */
#define INCLUDE_vTaskPrioritySet                1
#define INCLUDE_uxTaskPriorityGet               1
#define INCLUDE_vTaskDelete                     1
#define INCLUDE_vTaskSuspend                    1
#define INCLUDE_vTaskDelayUntil                 1
#define INCLUDE_vTaskDelay                      1
#define INCLUDE_xTaskGetSchedulerState          1
#define INCLUDE_xTaskGetCurrentTaskHandle       1
#define INCLUDE_uxTaskGetStackHighWaterMark     0
#define INCLUDE_xTaskGetIdleTaskHandle          1
#define INCLUDE_eTaskGetState                   1
#define INCLUDE_xTimerPendFunctionCall          1
#define INCLUDE_xTaskAbortDelay                 1
#define INCLUDE_xTaskGetHandle                  1

/* ---- Assertions ----
 * Routed to the board hook so a failed assert names its file and line on
 * stderr, rather than trapping with no message. */
extern void freertos_assert_failed(const char *file, int line);
#define configASSERT(x)                                                                            \
    do {                                                                                           \
        if (!(x)) {                                                                                \
            freertos_assert_failed(__FILE__, __LINE__);                                            \
        }                                                                                          \
    } while (0)

#endif /* FREERTOS_CONFIG_H */
