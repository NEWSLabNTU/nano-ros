/*
 * Phase 536 W2 (RFC-0052 "Names") -- a scheduling tier is a sched task. The
 * C ABI's tier names were renamed; the old ones stay one release as
 * deprecated aliases. This TU pins both halves:
 *   1. the new `sched_task` spelling compiles, and
 *   2. the old `tier` spelling still compiles, each use warning with the new
 *      name (the lane greps the warnings), resolving to the same functions.
 *
 * Compile-only (no main): never linked, never run.
 */

#include "rclc/detail/main.h"

/* 1. The new spelling. */
int32_t new_spelling(const nros_native_sched_task_spec_t* t, size_t n, void* storage,
                     size_t stride, const nros_sched_task_backing_t* b,
                     const nros_sched_task_memory_t* m);
int32_t new_spelling(const nros_native_sched_task_spec_t* t, size_t n, void* storage,
                     size_t stride, const nros_sched_task_backing_t* b,
                     const nros_sched_task_memory_t* m) {
    int32_t r = nros_board_native_run_sched_tasks("s", t, n);
    r |= nros_board_native_run_sched_tasks_ns("s", "/ns", t, n);
    r |= nros_board_native_run_sched_tasks_in("s", "/ns", t, n, storage, stride);
    r |= nros_board_freertos_run_sched_tasks_memory_in("l", 0, "s", "/ns", t, n, storage, stride, m);
    r |= nros_board_zephyr_run_sched_tasks_memory_in("l", 0, "s", "/ns", t, n, storage, stride, m);
    r |= nros_board_zephyr_run_sched_tasks_backed_in("l", 0, "s", "/ns", t, n, storage, stride, b,
                                                     m);
    return r;
}

/* 2. The old spelling: the same calls through the deprecated aliases. The
 * lane also builds this TU with them dropped, -Werror, which pins that the
 * new spelling stands alone. */
#ifndef NROS_NO_DEPRECATED_TIER_NAMES
int32_t old_spelling(const nros_native_tier_spec_t* t, size_t n, void* storage, size_t stride,
                     const nros_tier_backing_t* b, const nros_tier_task_memory_t* m);
int32_t old_spelling(const nros_native_tier_spec_t* t, size_t n, void* storage, size_t stride,
                     const nros_tier_backing_t* b, const nros_tier_task_memory_t* m) {
    int32_t r = nros_board_native_run_tiers("s", t, n);
    r |= nros_board_native_run_tiers_ns("s", "/ns", t, n);
    r |= nros_board_native_run_tiers_in("s", "/ns", t, n, storage, stride);
    r |= nros_board_freertos_run_tiers_tasks_in("l", 0, "s", "/ns", t, n, storage, stride, m);
    r |= nros_board_zephyr_run_tiers_tasks_in("l", 0, "s", "/ns", t, n, storage, stride, m);
    r |= nros_board_zephyr_run_tiers_backed_in("l", 0, "s", "/ns", t, n, storage, stride, b, m);
    return r;
}
#endif /* NROS_NO_DEPRECATED_TIER_NAMES */
