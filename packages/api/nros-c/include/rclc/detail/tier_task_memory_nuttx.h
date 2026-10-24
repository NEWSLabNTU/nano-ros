/*
 * tier_task_memory_nuttx.h — phase-509 P10: how a generated NuttX entry spells
 * one level's task memory.
 *
 * The entry declares each SPAWNED level's stack as a static and hands it to
 * `nros_board_nuttx_run_levels_in` as an `nros_tier_task_memory_t` row (see
 * <rclc/detail/main.h>); the runner gives it to the level's pthread with
 * `pthread_attr_setstack`. Before this every tier stack was a kernel
 * allocation of the declared size (or a 16 KiB default), invisible to the link
 * and to `mem-report`. NuttX has no control block to hand over: the row's
 * `tcb` is NULL and the stack is the whole of it.
 *
 * A level that declares no `stack_bytes` gets `NROS_LEVEL_STACK_BYTES(0)`: the
 * per-callback default plus the worker's own frame, which together are
 * NROS_NUTTX_LEVEL_STACK_DEFAULT, the 16 KiB every tier got before. Zephyr's
 * and FreeRTOS's headers state the same rule over their own knobs.
 *
 * Included only by a generated NuttX tiered entry.
 */
#ifndef NROS_TIER_TASK_MEMORY_NUTTX_H
#define NROS_TIER_TASK_MEMORY_NUTTX_H

#include <stddef.h>
#include <stdint.h>

#include <rclc/detail/main.h>

#ifndef NROS_NUTTX_LEVEL_STACK_DEFAULT
#define NROS_NUTTX_LEVEL_STACK_DEFAULT 16384u
#endif
/* Issue 2019 — the frame is measured, not estimated: a spawned level runs its
 * own setup (open_level, the members' registration) on its own stack before
 * its first pass, and that setup is the stack's peak. The C level runner's
 * spawned level on QEMU virt cortex-a7 (realtime-c, stack coloration) peaked
 * at 10,384 B after setup and no deeper after 3,000 passes; rounded up to the
 * next KiB. */
#ifndef NROS_NUTTX_LEVEL_WORKER_FRAME
#define NROS_NUTTX_LEVEL_WORKER_FRAME 11264u
#endif
#if NROS_NUTTX_LEVEL_WORKER_FRAME >= NROS_NUTTX_LEVEL_STACK_DEFAULT
#error "nros: NROS_NUTTX_LEVEL_WORKER_FRAME must stay below NROS_NUTTX_LEVEL_STACK_DEFAULT"
#endif
#define NROS_LEVEL_CALLBACK_STACK_DEFAULT_                                                         \
    (NROS_NUTTX_LEVEL_STACK_DEFAULT - NROS_NUTTX_LEVEL_WORKER_FRAME)
#define NROS_LEVEL_STACK_BYTES(member)                                                             \
    (((member) > 0u ? (member) : NROS_LEVEL_CALLBACK_STACK_DEFAULT_) +                             \
     NROS_NUTTX_LEVEL_WORKER_FRAME)

/* A level's stack in bytes: its declaration, rounded up to 16 (the strictest
 * stack alignment the supported architectures ask). */
#define NROS_TIER_TASK_STACK_BYTES_(bytes) ((((size_t)(bytes)) + 15u) & ~(size_t)15u)

/* Level `i`'s stack, as a file-scope static named for `mem-report`. */
#define NROS_TIER_TASK_MEMORY_DEFINE(i, bytes)                                                     \
    static uint8_t __nros_tier_stack_##i[NROS_TIER_TASK_STACK_BYTES_(bytes)]                       \
        __attribute__((aligned(16)))

/* Level `i`'s row of the `nros_tier_task_memory_t` array. */
#define NROS_TIER_TASK_MEMORY(i)                                                                   \
    { (void*)__nros_tier_stack_##i, sizeof(__nros_tier_stack_##i), NULL }

/* The boot level's row: it runs on the caller's thread and has no memory here. */
#define NROS_TIER_TASK_MEMORY_NONE                                                                 \
    { NULL, 0u, NULL }

#endif /* NROS_TIER_TASK_MEMORY_NUTTX_H */
