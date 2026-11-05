/**
 * @file tier_task_memory_zephyr.h
 * @brief Deprecated forwarding header: renamed <rclc/detail/sched_task_memory_zephyr.h>.
 *
 * Phase 536 W2 (RFC-0052 "Names"): a scheduling tier is a sched task. This
 * header and the three macros below stay one release so an entry generated
 * before the rename still builds, with a warning naming the new spelling;
 * define `NROS_NO_DEPRECATED_TIER_NAMES` to drop the macros early.
 */
#ifndef NROS_TIER_TASK_MEMORY_ZEPHYR_H
#define NROS_TIER_TASK_MEMORY_ZEPHYR_H

#if defined(__GNUC__) || defined(__clang__)
#warning                                                                                           \
    "<rclc/detail/tier_task_memory_zephyr.h> is renamed <rclc/detail/sched_task_memory_zephyr.h> (phase 536)"
#endif

#include <rclc/detail/sched_task_memory_zephyr.h>

#ifndef NROS_NO_DEPRECATED_TIER_NAMES
#define NROS_TIER_TASK_MEMORY_DEFINE(i, bytes) NROS_SCHED_TASK_MEMORY_DEFINE(i, bytes)
#define NROS_TIER_TASK_MEMORY(i) NROS_SCHED_TASK_MEMORY(i)
#define NROS_TIER_TASK_MEMORY_NONE NROS_SCHED_TASK_MEMORY_NONE
#endif /* NROS_NO_DEPRECATED_TIER_NAMES */

#endif /* NROS_TIER_TASK_MEMORY_ZEPHYR_H */
