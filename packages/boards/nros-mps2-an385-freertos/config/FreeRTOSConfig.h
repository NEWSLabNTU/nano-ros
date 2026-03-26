/*
 * FreeRTOS kernel configuration for QEMU MPS2-AN385 (Cortex-M3)
 *
 * Tuned for nros + zenoh-pico + lwIP:
 *   - Recursive mutexes (zenoh-pico)
 *   - Dynamic allocation (lwIP sys_arch, zenoh-pico)
 *   - Timer service (lwIP timeouts)
 *   - 25 MHz CPU clock (QEMU MPS2-AN385 default)
 */

#ifndef FREERTOS_CONFIG_H
#define FREERTOS_CONFIG_H

/* ---- Scheduler ---- */
#define configUSE_PREEMPTION                    1
#define configUSE_PORT_OPTIMISED_TASK_SELECTION 0
#define configUSE_TICKLESS_IDLE                 0
#define configCPU_CLOCK_HZ                      ((unsigned long)25000000)
#define configTICK_RATE_HZ                      ((TickType_t)1000)
#define configMAX_PRIORITIES                    8
#define configMINIMAL_STACK_SIZE                ((unsigned short)256)
#define configMAX_TASK_NAME_LEN                 16
#define configUSE_16_BIT_TICKS                  0
#define configIDLE_SHOULD_YIELD                 1
#define configTASK_NOTIFICATION_ARRAY_ENTRIES   3

/* ---- Memory ---- */
#define configSUPPORT_STATIC_ALLOCATION         0
#define configSUPPORT_DYNAMIC_ALLOCATION        1
#define configTOTAL_HEAP_SIZE                   ((size_t)(256 * 1024))
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

/* ---- Cortex-M3 interrupt priorities ---- */
/* MPS2-AN385 uses 3 priority bits (8 levels) */
#ifdef __NVIC_PRIO_BITS
    #define configPRIO_BITS __NVIC_PRIO_BITS
#else
    #define configPRIO_BITS 3
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
#define configUSE_MALLOC_FAILED_HOOK            0
#define configCHECK_FOR_STACK_OVERFLOW          0

/* ---- Tonbandgeraet tracing (opt-in via NROS_TRACE=1) ---- */
#ifdef NROS_TRACE
#define configUSE_TRACE_FACILITY                1
#include "tband.h"
#endif

#endif /* FREERTOS_CONFIG_H */
