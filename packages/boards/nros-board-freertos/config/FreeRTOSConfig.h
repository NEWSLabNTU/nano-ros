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
/* Issue 1598 — ON, so each tier task's stack and TCB are statics the generated
 * entry owns (`xTaskCreateStatic`), sized by the build, placed and priced by the
 * linker, named by `mem-report`. The two hooks this obliges the application to
 * supply are in `c/freertos_hooks.c`. Dynamic allocation stays on: every other
 * task (app, zenoh read/lease, poll) is still `xTaskCreate`. */
#define configSUPPORT_STATIC_ALLOCATION         1
#define configSUPPORT_DYNAMIC_ALLOCATION        1
/* Phase 175.B / 204.6 — FreeRTOS heap (heap_4 `ucHeap[]`, the dominant bss).
 *
 * WHO GETS WHICH NUMBER — three roads, decided by WHERE the RMW is known.
 *
 * 1. CARGO road, `rmw-zenoh` on: the board build.rs passes
 *    `-DNROS_FREERTOS_HEAP_KB` from the measured derivation
 *    `nros_board_common::freertos_config::default_heap_bytes`. Nothing below
 *    applies (an explicit size is never adjusted).
 * 2. CMAKE road (every C/C++ image: the kernel is `freertos_kernel`, which never
 *    runs the board build.rs), Cyclone or XRCE: `cmake/platform/
 *    nano-ros-freertos.cmake` defines `NROS_FREERTOS_HEAP_DEFAULT_DDS`, and the
 *    heap is `NROS_FREERTOS_DDS_HEAP_KB` below — issue 1624's MEASURED
 *    derivation, `default_dds_heap_bytes(C_CARRIER_APP_STACK_BYTES)` = 65,536
 *    (the carrier's app stack) + 589,824 (the DDS working set) = 640 KiB. A C
 *    header cannot call the Rust function, so the literal is held to it by
 *    `freertos_config::tests::the_header_states_the_derived_dds_default`.
 *    MEASURED on `workspace-cpp-mps3-an536-freertos` (the S32Z270 Cyclone
 *    entry's emulated twin: same C++ entry, Cortex-R52, kernel port) on qemu
 *    mps3-an536, 2026-10-02: `nros: heap peak 447944 of 33554432 bytes`, i.e.
 *    382,408 beside the app stack; the term is 1.54x that (no remote
 *    participant was on the LAN). Kept measured by
 *    `freertos_qemu::an536_cyclonedds_cpp_entry_delivers_within_the_dds_heap_default`.
 *    NO tier-stack subtraction here: that image has no tiers, and on this road
 *    tier stacks (1598) and tier executors (1568) are `.bss`, so the derivation
 *    never contained them — subtracting would take them out twice.
 * 3. Everything else — CMAKE road on zenoh, and a cargo build of the family
 *    crate with no RMW feature — takes the FALLBACK, 3072 KiB minus the tier
 *    stacks the entry moved to `.bss` (issue 1598): those stacks came out of
 *    this budget until 1598 gave each spawned tier a static one, so without
 *    the subtraction the bytes were reserved twice (realtime-cpp's
 *    `demo_bringup:freertos` overflowed RAM by 151,024 bytes with `ucHeap` at
 *    0x300000). `nano_ros_entry` defines `NROS_FREERTOS_TIER_STACKS_IN_BSS_KB`
 *    from the generated entry's own declarations. The zenoh cmake images were
 *    NOT re-derived: on this host they open a session and never tick, on main
 *    as on this change, so no heap peak past session open could be read
 *    (issue 1657) — a cut nobody could measure is not made here.
 *
 * Override per image with `NROS_FREERTOS_HEAP_KB` (cargo: the build env, which
 * build.rs forwards; cmake: a compile definition). It is a size someone chose
 * and is never adjusted by either rule above. */
#ifndef NROS_FREERTOS_TIER_STACKS_IN_BSS_KB
#define NROS_FREERTOS_TIER_STACKS_IN_BSS_KB     0
#endif
#define NROS_FREERTOS_DDS_HEAP_KB 640
#ifndef NROS_FREERTOS_HEAP_KB
#if defined(NROS_FREERTOS_HEAP_DEFAULT_DDS) && NROS_FREERTOS_HEAP_DEFAULT_DDS
#define NROS_FREERTOS_HEAP_KB                   (NROS_FREERTOS_DDS_HEAP_KB)
#else
#define NROS_FREERTOS_HEAP_KB                   (3072 - (NROS_FREERTOS_TIER_STACKS_IN_BSS_KB))
#endif
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
