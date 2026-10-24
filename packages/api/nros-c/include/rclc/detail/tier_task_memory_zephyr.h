/*
 * tier_task_memory_zephyr.h — issue 1232: how a generated Zephyr entry spells
 * one tier task's memory.
 *
 * The entry declares each SPAWNED tier's thread object and stack, the stack
 * with `K_THREAD_STACK_DEFINE` at the tier's own declared `stack_bytes`, and
 * hands them to `nros_board_zephyr_run_tiers_tasks_in` as
 * `nros_tier_task_memory_t` rows (see <rclc/detail/main.h>). Before this every tier
 * thread got the shim's fixed `NROS_ZEPHYR_TIER_STACK_SIZE` pool slot, whatever
 * it declared: a tier declaring more ran undersized behind a printk.
 *
 * A tier that declares nothing (`bytes` 0) gets the board default,
 * CONFIG_NROS_ZEPHYR_TIER_STACK_SIZE — the same number the pool slot used, so
 * the knob keeps its meaning for undeclared tiers. `K_THREAD_STACK_DEFINE`
 * applies the architecture's alignment, guard and reserved area; the row
 * carries `K_THREAD_STACK_SIZEOF`, the size the thread can use.
 *
 * Included only by a generated Zephyr tiered entry.
 */
#ifndef NROS_TIER_TASK_MEMORY_ZEPHYR_H
#define NROS_TIER_TASK_MEMORY_ZEPHYR_H

#include <stddef.h>

#include <zephyr/kernel.h>

#include <rclc/detail/main.h>

#ifdef CONFIG_NROS_ZEPHYR_TIER_STACK_SIZE
#define NROS_TIER_TASK_STACK_DEFAULT_ CONFIG_NROS_ZEPHYR_TIER_STACK_SIZE
#else
#define NROS_TIER_TASK_STACK_DEFAULT_ 16384
#endif

#define NROS_TIER_TASK_STACK_BYTES_(bytes) ((bytes) > 0u ? (bytes) : NROS_TIER_TASK_STACK_DEFAULT_)

/* phase-509 P9 (RFC-0108 D3) — a LEVEL's stack, derived from its members: the
 * largest member's stated stack, or the per-callback default for a member that
 * states none, plus the worker's own frame (the spin loop, spin_once and the
 * transport drive under it). Never the platform's thread default. The
 * per-callback default is the level knob minus the frame, so a level whose
 * members state nothing gets exactly CONFIG_NROS_ZEPHYR_TIER_STACK_SIZE, the
 * stack every tier got before. A generated entry passes a spawned level that
 * declares no `stack_bytes` as `NROS_LEVEL_STACK_BYTES(member)`, and the cost
 * report (`scripts/nros-mem-report.py --levels`) checks the static against the
 * same arithmetic.
 *
 * Issue 2019 -- the frame covers the level's own setup (open_level, the
 * members' registration), which runs on the level's stack before its first
 * pass and is that stack's peak on the ports measured (FreeRTOS, NuttX). The
 * 4,096 B default is an estimate on Zephyr: measure a board with
 * CONFIG_THREAD_ANALYZER and set CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME. */
#ifdef CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME
#define NROS_LEVEL_WORKER_FRAME_ CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME
#else
#define NROS_LEVEL_WORKER_FRAME_ 4096
#endif
#define NROS_LEVEL_CALLBACK_STACK_DEFAULT_                                                         \
    (NROS_TIER_TASK_STACK_DEFAULT_ - NROS_LEVEL_WORKER_FRAME_)
#define NROS_LEVEL_STACK_BYTES(member)                                                             \
    (((member) > 0u ? (member) : NROS_LEVEL_CALLBACK_STACK_DEFAULT_) + NROS_LEVEL_WORKER_FRAME_)
BUILD_ASSERT(NROS_LEVEL_WORKER_FRAME_ < NROS_TIER_TASK_STACK_DEFAULT_,
             "CONFIG_NROS_ZEPHYR_LEVEL_WORKER_FRAME must stay below "
             "CONFIG_NROS_ZEPHYR_TIER_STACK_SIZE: their difference is the per-callback default");

/* Tier `i`'s stack and thread object, as file-scope statics named for
 * `mem-report`. */
#define NROS_TIER_TASK_MEMORY_DEFINE(i, bytes)                                                     \
    static K_THREAD_STACK_DEFINE(__nros_tier_stack_##i, NROS_TIER_TASK_STACK_BYTES_(bytes));       \
    static struct k_thread __nros_tier_thread_##i

/* Tier `i`'s row of the `nros_tier_task_memory_t` array. */
#define NROS_TIER_TASK_MEMORY(i)                                                                   \
    {                                                                                              \
        (void*)__nros_tier_stack_##i, K_THREAD_STACK_SIZEOF(__nros_tier_stack_##i),                \
            (void*)&__nros_tier_thread_##i                                                         \
    }

/* The boot tier's row: it runs on the `main()` thread and has no memory here. */
#define NROS_TIER_TASK_MEMORY_NONE                                                                 \
    { NULL, 0u, NULL }

#endif /* NROS_TIER_TASK_MEMORY_ZEPHYR_H */
