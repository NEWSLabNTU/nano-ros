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

z_result_t _z_task_init(_z_task_t *task, z_task_attr_t *attr, void *(*fun)(void *), void *arg) {
    (void)attr;

    task->_fun = fun;
    task->_arg = arg;

    UINT status = tx_event_flags_create(&task->done_flags, "zdone");
    if (status != TX_SUCCESS) return _Z_ERR_GENERIC;

    status = tx_thread_create(
        &(task->threadx_thread), "ztask",
        _z_task_trampoline, 0,
        task->threadx_stack, Z_TASK_STACK_SIZE,
        Z_TASK_PRIORITY, Z_TASK_PREEMPT_THRESHOLD,
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
