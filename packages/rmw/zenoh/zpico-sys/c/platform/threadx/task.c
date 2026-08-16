/**
 * task.c — ThreadX task creation for zenoh-pico
 *
 * Provides _z_task_init/_z_task_join because they need access to the
 * _z_task_t struct layout (TX_THREAD + embedded stack + function/arg pointers).
 * All other platform symbols (clock, malloc, sleep, mutex, condvar, random)
 * are provided by zpico-platform-shim → nros-platform-threadx.
 */

#if defined(ZENOH_THREADX)

#include <stdio.h>
/* Phase 154 — `#undef NROS_PLATFORM_ALIASES` so the central
 * `zenoh-pico/system/platform.h` dispatcher picks
 * `c/platform/threadx/platform.h` (concrete TX_THREAD-flavoured
 * `_z_task_t { TX_THREAD t; void (*_fun)(void *); void *_arg;
 * TX_EVENT_FLAGS_GROUP done_flags; }`) instead of
 * `c/zpico/nros_zenoh_generic_platform.h` (opaque storage).
 * The vendor build defines `NROS_PLATFORM_ALIASES` so its
 * socket layout matches the alias TU; this file implements
 * `_z_task_init` / `_z_task_join` against the concrete struct
 * fields and is the only known TU that needs the per-RTOS task
 * layout. The `#undef` is TU-local — other vendor TUs keep
 * the alias-flavoured layout. */
#undef NROS_PLATFORM_ALIASES
#include "zenoh-pico/system/platform.h"

/* ── Task trampoline ───────────────────────────────────────────────────── */

/*
 * ThreadX entry functions receive ULONG (32-bit on x86_64). We store the
 * real function+arg in the _z_task_t struct and recover via tx_thread_identify().
 */
/*
 * Phase 77.21: trampoline signals bit 0 of the task's event-flags group
 * after `_fun` returns so that `_z_task_join` can wake immediately via
 * `tx_event_flags_get(..., TX_WAIT_FOREVER)` instead of polling
 * `tx_thread_info_get` + `tx_thread_sleep(1)` on every tick.
 */
#define _Z_TASK_DONE_FLAG 0x1u

static void _z_task_trampoline(ULONG input) {
    (void)input;
    TX_THREAD *tcb = tx_thread_identify();
    _z_task_t *task = (_z_task_t *)tcb;
    if (task && task->_fun) {
        task->_fun(task->_arg);
    }
    if (task) {
        tx_event_flags_set(&task->done_flags, _Z_TASK_DONE_FLAG, TX_OR);
    }
}

/* Issue 0626 — normalised 0-31 band -> ThreadX priority.
 *
 * The two scales run in OPPOSITE directions, which is the entire reason
 * phase-364 W5 introduced the band: ThreadX documents priority as "0 through
 * (TX_MAX_PRIORITIES-1), where a value of 0 represents the highest priority",
 * while the band is 0 = least urgent, larger = more urgent. A number carried
 * across without inverting means "run me first" on one kernel and "run me
 * last" on another — exactly the bug the band exists to prevent.
 *
 * Scaled against TX_MAX_PRIORITIES rather than a literal 32: it is
 * configurable (32..1024, divisible by 32) and this file cannot assume the
 * default. */
static UINT _z_task_threadx_priority(int32_t normalized) {
    const UINT levels = (UINT)TX_MAX_PRIORITIES;         /* >= 32 */
    const UINT lowest = levels - 1u;                     /* numerically largest */
    uint32_t n = normalized < 0 ? 0u : (uint32_t)normalized;
    if (n > 31u) {
        n = 31u;
    }
    /* Round-to-nearest across the band, then INVERT. */
    const UINT scaled = (UINT)((lowest * n * 2u + 31u) / 62u);
    return lowest - scaled;
}

z_result_t _z_task_init(_z_task_t *task, z_task_attr_t *attr, void *(*fun)(void *), void *arg) {
    /* Issue 0626 — `attr` used to be discarded, so every zenoh task (read,
     * lease, tx-flush) ran at the single compile-time `Z_TASK_PRIORITY` and
     * `zpico_set_task_config`'s per-task values reached nothing. A NULL still
     * means "every default", as on every other port. */
    UINT priority = Z_TASK_PRIORITY;
    const char *name = "ztask";
    if (attr != NULL) {
        if (attr->priority != NROS_PLATFORM_PRIORITY_INHERIT) {
            priority = NROS_PLATFORM_PRIORITY_IS_RAW(attr->priority)
                           ? (UINT)NROS_PLATFORM_PRIORITY_RAW_VALUE(attr->priority)
                           : _z_task_threadx_priority(attr->priority);
        }
        if (attr->name != NULL) {
            name = attr->name;
        }
        /* `stack_bytes` is deliberately NOT honoured: the stack is EMBEDDED in
         * `_z_task_t` at the compile-time `Z_TASK_STACK_SIZE`, so there is no
         * larger region to point at. Silently accepting a bigger number would
         * be worse than ignoring it. */
    }
    if (priority > (UINT)(TX_MAX_PRIORITIES - 1)) {
        priority = (UINT)(TX_MAX_PRIORITIES - 1);
    }

    task->_fun = fun;
    task->_arg = arg;

    UINT status = tx_event_flags_create(&task->done_flags, "zdone");
    if (status != TX_SUCCESS) return _Z_ERR_GENERIC;

    /* preempt_threshold must be <= priority numerically ("only priorities
     * higher than this level are allowed to preempt"). Tracking the resolved
     * priority keeps the previous behaviour, where both were Z_TASK_PRIORITY;
     * leaving the constant here would make an attr-supplied priority ILLEGAL
     * whenever it resolved below the fixed threshold, and tx_thread_create
     * would fail with TX_PRIORITY_ERROR. */
    status = tx_thread_create(
        &(task->threadx_thread), (CHAR *)name,
        _z_task_trampoline, 0,
        task->threadx_stack, Z_TASK_STACK_SIZE,
        priority, priority,
        Z_TASK_TIME_SLICE, TX_AUTO_START);
    if (status != TX_SUCCESS) {
        tx_event_flags_delete(&task->done_flags);
        return _Z_ERR_GENERIC;
    }
    return _Z_RES_OK;
}

z_result_t _z_task_join(_z_task_t *task) {
    ULONG actual_flags;
    UINT status = tx_event_flags_get(
        &task->done_flags, _Z_TASK_DONE_FLAG, TX_OR_CLEAR,
        &actual_flags, TX_WAIT_FOREVER);
    if (status != TX_SUCCESS) return _Z_ERR_GENERIC;
    return _Z_RES_OK;
}

z_result_t _z_task_detach(_z_task_t *task) {
    (void)task;
    return _Z_ERR_GENERIC;
}

z_result_t _z_task_cancel(_z_task_t *task) {
    (void)task;
    return _Z_ERR_GENERIC;
}

void _z_task_exit(void) {
    /* ThreadX threads terminate when they return from their entry function. */
}

void _z_task_free(_z_task_t **task) {
    if (*task) {
        /* Phase 77.21: release the event-flags group allocated in `_z_task_init`. */
        tx_event_flags_delete(&(*task)->done_flags);
    }
    z_free(*task);
    *task = NULL;
}

#endif /* ZENOH_THREADX */
