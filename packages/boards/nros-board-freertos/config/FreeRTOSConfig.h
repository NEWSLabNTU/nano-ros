/*
 * Shared FreeRTOS kernel configuration for nano-ros boards (Cortex-M + lwIP).
 *
 * phase-337 W5.a — this file used to be per-board. Of its 111 lines exactly
 * TWO were board facts (`configCPU_CLOCK_HZ`, `configPRIO_BITS`), so the file
 * moved here and the board now supplies the two NUMBERS instead of a copy of
 * the file. A board's `config/FreeRTOSConfig.h` is:
 *
 *     #define NROS_BOARD_CPU_CLOCK_HZ 25000000
 *     #define NROS_BOARD_PRIO_BITS    3
 *     #include "../../nros-board-freertos/config/FreeRTOSConfig.h"
 *
 * The include is RELATIVE ON PURPOSE. `FREERTOS_CONFIG_DIR` is a single
 * directory read by six build scripts plus the CMake lane, so making it a
 * search PATH would have been a cross-cutting change; a relative `#include "…"`
 * resolves against the including file's own directory and therefore works
 * identically in both lanes with no include-path edit at all. An out-of-tree
 * board that cannot spell that path takes RFC-0064 ladder rung 3 and owns the
 * whole file — the rung-3 rule is why that is an acceptable fallback.
 *
 * Tuned for nros + zenoh-pico + lwIP:
 *   - Recursive mutexes (zenoh-pico)
 *   - Dynamic allocation (lwIP sys_arch, zenoh-pico)
 *   - Timer service (lwIP timeouts)
 */

#ifndef FREERTOS_CONFIG_H
#define FREERTOS_CONFIG_H

#ifndef NROS_BOARD_CPU_CLOCK_HZ
#error "board must #define NROS_BOARD_CPU_CLOCK_HZ before including this file"
#endif
#if !defined(NROS_BOARD_PRIO_BITS) && !defined(__NVIC_PRIO_BITS)
#error "board must #define NROS_BOARD_PRIO_BITS (or supply a CMSIS __NVIC_PRIO_BITS)"
#endif

/* ---- Scheduler ---- */
#define configUSE_PREEMPTION                    1
#define configUSE_PORT_OPTIMISED_TASK_SELECTION 0
#define configUSE_TICKLESS_IDLE                 0
#define configCPU_CLOCK_HZ                      ((unsigned long)NROS_BOARD_CPU_CLOCK_HZ)
#define configTICK_RATE_HZ                      ((TickType_t)1000)
#define configMAX_PRIORITIES                    8
#define configMINIMAL_STACK_SIZE                ((unsigned short)256)
#define configSTACK_DEPTH_TYPE                  uint32_t
#define configMAX_TASK_NAME_LEN                 16
#define configUSE_16_BIT_TICKS                  0
#define configIDLE_SHOULD_YIELD                 1
#define configTASK_NOTIFICATION_ARRAY_ENTRIES   3

/* ---- Memory ---- */
#define configSUPPORT_STATIC_ALLOCATION         0
#define configSUPPORT_DYNAMIC_ALLOCATION        1
/* Phase 175.B / 204.6 — FreeRTOS heap (heap_4 `ucHeap[]`, the dominant bss).
 * CycloneDDS participant startup creates the builtin discovery endpoints plus
 * lwIP socket semaphores before the Rust talker can publish, so the default is
 * sized for that heavy boot path (within the 4 MiB MPS2-AN385 SRAM budget).
 * Lighter RMWs (zenoh-pico ~12 KB working set, XRCE static pools) override it
 * per-example via the build env `NROS_FREERTOS_HEAP_KB` (the kernel build.rs
 * forwards it as `-DNROS_FREERTOS_HEAP_KB`), e.g. `[env] NROS_FREERTOS_HEAP_KB
 * = "256"` in the example's `.cargo/config.toml`. Tune to the RMW's measured
 * high-water (`xPortGetMinimumEverFreeHeapSize()`); default stays cyclone-safe. */
#ifndef NROS_FREERTOS_HEAP_KB
#define NROS_FREERTOS_HEAP_KB                   3072
#endif
#define configTOTAL_HEAP_SIZE                   ((size_t)((NROS_FREERTOS_HEAP_KB) * 1024))
#define configAPPLICATION_ALLOCATED_HEAP        0

/* ---- Synchronisation ---- */
#define configUSE_MUTEXES                       1
#define configUSE_RECURSIVE_MUTEXES             1
#define configUSE_COUNTING_SEMAPHORES           1
#define configQUEUE_REGISTRY_SIZE               10

/* ---- Timers ---- */
#define configUSE_TIMERS                        1
#define configTIMER_TASK_PRIORITY               2
#define configTIMER_QUEUE_LENGTH                10
#define configTIMER_TASK_STACK_DEPTH            (configMINIMAL_STACK_SIZE * 2)

/* ---- Optional API functions ---- */
#define INCLUDE_vTaskPrioritySet                1
#define INCLUDE_uxTaskPriorityGet               1
#define INCLUDE_vTaskDelete                     1
#define INCLUDE_vTaskSuspend                    1
#define INCLUDE_xResumeFromISR                  1
#define INCLUDE_vTaskDelayUntil                 1
#define INCLUDE_vTaskDelay                      1
#define INCLUDE_xTaskGetSchedulerState          1
#define INCLUDE_xTaskGetCurrentTaskHandle       1
#define INCLUDE_uxTaskGetStackHighWaterMark     1
#define INCLUDE_xTaskGetIdleTaskHandle          1
#define INCLUDE_eTaskGetState                   1
#define INCLUDE_xTimerPendFunctionCall          1

/* ---- Cortex-M interrupt priorities ---- */
/* NVIC priority bits are a board fact (MPS2-AN385: 3 bits / 8 levels). A CMSIS
 * device header, when one is on the include path, states it authoritatively. */
#ifdef __NVIC_PRIO_BITS
    #define configPRIO_BITS __NVIC_PRIO_BITS
#else
    #define configPRIO_BITS NROS_BOARD_PRIO_BITS
#endif

#define configLIBRARY_LOWEST_INTERRUPT_PRIORITY         7
#define configLIBRARY_MAX_SYSCALL_INTERRUPT_PRIORITY    5
#define configKERNEL_INTERRUPT_PRIORITY \
    (configLIBRARY_LOWEST_INTERRUPT_PRIORITY << (8 - configPRIO_BITS))
#define configMAX_SYSCALL_INTERRUPT_PRIORITY \
    (configLIBRARY_MAX_SYSCALL_INTERRUPT_PRIORITY << (8 - configPRIO_BITS))

/* ---- Assert ---- */
/* Semihosting-compatible assert for QEMU debugging */
extern void freertos_assert_failed(const char *file, int line);
#define configASSERT(x)                                     \
    if ((x) == 0) { freertos_assert_failed(__FILE__, __LINE__); }

/* ---- Hook functions ---- */
#define configUSE_IDLE_HOOK                     1
#ifdef NROS_TRACE
#define configUSE_TICK_HOOK                     1
#else
#define configUSE_TICK_HOOK                     0
#endif
#define configUSE_MALLOC_FAILED_HOOK            1
#define configCHECK_FOR_STACK_OVERFLOW          2
#define configNUM_THREAD_LOCAL_STORAGE_POINTERS 1

/* ---- Tonbandgeraet tracing (opt-in via NROS_TRACE=1) ---- */
#ifdef NROS_TRACE
#define configUSE_TRACE_FACILITY                1
#include "tband.h"
#endif

#endif /* FREERTOS_CONFIG_H */
