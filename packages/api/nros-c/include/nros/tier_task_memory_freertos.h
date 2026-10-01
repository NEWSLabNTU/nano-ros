/*
 * tier_task_memory_freertos.h — issue 1598: how a generated FreeRTOS entry
 * spells one tier task's memory.
 *
 * The entry declares each SPAWNED tier's stack and TCB as statics and hands
 * them to `nros_board_freertos_run_tiers_tasks_in` as an
 * `nros_tier_task_memory_t` row (see <nros/main.h>), which `xTaskCreateStatic`s
 * the tier over them. Before this every tier stack was an `xTaskCreate` block
 * of heap_4 — 256 KiB per tier by default, invisible to the link and to
 * `mem-report`, and an image that could not hold them failed at BOOT as
 * `*** MALLOC FAILED ***` instead of at link.
 *
 * The size is the tier's declared `stack_bytes`, or the default the emitter
 * states (it moved there from the runner, so the number sits where it is
 * reserved). The PORT's floor still applies (issue 0667: a task's size is a
 * floor the port raises, never one the caller can get right): a declaration
 * below `configMINIMAL_STACK_SIZE` words is raised to it here, at compile
 * time, because a static buffer cannot be raised at run time.
 *
 * Included only by a generated FreeRTOS tiered entry, which is compiled with
 * the board's FreeRTOS include path.
 */
#ifndef NROS_TIER_TASK_MEMORY_FREERTOS_H
#define NROS_TIER_TASK_MEMORY_FREERTOS_H

#include <stddef.h>

#include "FreeRTOS.h"
#include "task.h"

#include <nros/main.h>

#if !defined(configSUPPORT_STATIC_ALLOCATION) || (configSUPPORT_STATIC_ALLOCATION != 1)
#error                                                                                             \
    "nros: a tiered FreeRTOS entry owns its tier stacks (issue 1598), which needs configSUPPORT_STATIC_ALLOCATION 1 in FreeRTOSConfig.h"
#endif

/* Words for a tier declaring `bytes`, rounded UP and raised to the port's
 * floor. */
#define NROS_TIER_TASK_STACK_WORDS_(bytes)                                                         \
    ((((bytes) + sizeof(StackType_t) - 1u) / sizeof(StackType_t)) <                                \
             (size_t)configMINIMAL_STACK_SIZE                                                      \
         ? (size_t)configMINIMAL_STACK_SIZE                                                        \
         : (((bytes) + sizeof(StackType_t) - 1u) / sizeof(StackType_t)))

/* Tier `i`'s stack and TCB, as file-scope statics named for `mem-report`. */
#define NROS_TIER_TASK_MEMORY_DEFINE(i, bytes)                                                     \
    static StackType_t __nros_tier_stack_##i[NROS_TIER_TASK_STACK_WORDS_(bytes)];                  \
    static StaticTask_t __nros_tier_tcb_##i

/* Tier `i`'s row of the `nros_tier_task_memory_t` array. */
#define NROS_TIER_TASK_MEMORY(i)                                                                   \
    { (void*)__nros_tier_stack_##i, sizeof(__nros_tier_stack_##i), (void*)&__nros_tier_tcb_##i }

/* The boot tier's row: it runs on the caller's task and has no memory here. */
#define NROS_TIER_TASK_MEMORY_NONE                                                                 \
    { NULL, 0u, NULL }

#endif /* NROS_TIER_TASK_MEMORY_FREERTOS_H */
