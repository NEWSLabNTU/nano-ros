/*
 * tier_task_memory_freertos.h — issue 1598: how a generated FreeRTOS entry
 * spells one tier task's memory.
 *
 * The entry declares each SPAWNED tier's stack and TCB as statics and hands
 * them to `nros_board_freertos_run_tiers_tasks_in` as an
 * `nros_tier_task_memory_t` row (see <rclc/detail/main.h>), which `xTaskCreateStatic`s
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

#include <rclc/detail/main.h>

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

/* phase-509 P10 (RFC-0108 D3) — a LEVEL's stack, derived from its members: the
 * largest member's stated stack, or the per-callback default for a member that
 * states none, plus the worker's own frame (the spin loop, spin_once and the
 * transport drive under it). Never the platform's task default. The
 * per-callback default is the level default minus the frame, so a level whose
 * members state nothing gets exactly NROS_FREERTOS_LEVEL_STACK_DEFAULT, the
 * 256 KiB every spawned tier got before (issue #126). A generated entry passes
 * a spawned level that declares no `stack_bytes` as
 * `NROS_LEVEL_STACK_BYTES(member)`. Zephyr's header states the same rule over
 * its own knobs. */
#ifndef NROS_FREERTOS_LEVEL_STACK_DEFAULT
#define NROS_FREERTOS_LEVEL_STACK_DEFAULT 262144u
#endif
/* Issue 2019 -- the frame is measured, not estimated: a spawned level runs its
 * own setup (open_level, the members' registration) on its own stack before
 * its first pass, and that setup is the stack's peak. The C level runner's
 * spawned level on mps2-an385 QEMU (realtime-c) peaked at 9,448 B after setup
 * and no deeper after 3,000 passes; rounded up to the next KiB. */
#ifndef NROS_FREERTOS_LEVEL_WORKER_FRAME
#define NROS_FREERTOS_LEVEL_WORKER_FRAME 10240u
#endif
#define NROS_LEVEL_CALLBACK_STACK_DEFAULT_                                                         \
    (NROS_FREERTOS_LEVEL_STACK_DEFAULT - NROS_FREERTOS_LEVEL_WORKER_FRAME)
#define NROS_LEVEL_STACK_BYTES(member)                                                             \
    (((member) > 0u ? (member) : NROS_LEVEL_CALLBACK_STACK_DEFAULT_) +                             \
     NROS_FREERTOS_LEVEL_WORKER_FRAME)
#if NROS_FREERTOS_LEVEL_WORKER_FRAME >= NROS_FREERTOS_LEVEL_STACK_DEFAULT
#error "nros: NROS_FREERTOS_LEVEL_WORKER_FRAME must stay below NROS_FREERTOS_LEVEL_STACK_DEFAULT"
#endif

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
