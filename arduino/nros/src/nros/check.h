/**
 * @file check.h
 * @ingroup grp_types
 * @brief Error-check convenience macros for the nros C API.
 *
 * Wraps the common "call returns nros_ret_t; on failure log and bail"
 * pattern in one-liners so example main()s and user code don't open-code
 * 4-line check blocks at every API site.
 *
 * Pattern parity with `rclc`:
 *   - `NROS_CHECK(call)`     — fatal: log and `return` from the
 *                              enclosing void function.
 *   - `NROS_SOFTCHECK(call)` — non-fatal: log and continue.
 *
 * The macros log file:line, the literal call text, and the integer
 * return code via `printf`. Linkage to `printf` is the user's
 * responsibility (typically already pulled in by examples).
 *
 * Override the log function by defining `NROS_CHECK_LOG(file, line, expr,
 * ret)` before including this header (e.g. to route through a board's
 * UART, RTT, or `nros-log` once Phase 88 lands).
 *
 * Copyright 2026 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_CHECK_H
#define NROS_CHECK_H

#include "nros/types.h"

#ifndef NROS_CHECK_LOG
#include <stdio.h>
#define NROS_CHECK_LOG(file, line, expr, ret)                                                      \
    printf("[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))
#endif

/**
 * Fatal check: evaluate @p call once; if the returned `nros_ret_t` is
 * not `NROS_RET_OK`, log the failure and `return` from the enclosing
 * function (which must return `void` or accept a bare `return;`).
 */
#define NROS_CHECK(call)                                                                           \
    do {                                                                                           \
        nros_ret_t _nros_check_ret = (call);                                                       \
        if (_nros_check_ret != NROS_RET_OK) {                                                      \
            NROS_CHECK_LOG(__FILE__, __LINE__, #call, _nros_check_ret);                            \
            return;                                                                                \
        }                                                                                          \
    } while (0)

/**
 * Non-fatal check: evaluate @p call once; on failure, log and continue.
 * Use for "best effort" calls (e.g. publishing a heartbeat) where one
 * dropped iteration shouldn't tear down the node.
 */
#define NROS_SOFTCHECK(call)                                                                       \
    do {                                                                                           \
        nros_ret_t _nros_softcheck_ret = (call);                                                   \
        if (_nros_softcheck_ret != NROS_RET_OK) {                                                  \
            NROS_CHECK_LOG(__FILE__, __LINE__, #call, _nros_softcheck_ret);                        \
        }                                                                                          \
    } while (0)

/**
 * Like `NROS_CHECK` but for callers that need a custom return value
 * (non-`void` functions). Returns @p retval on failure.
 */
#define NROS_CHECK_RET(call, retval)                                                               \
    do {                                                                                           \
        nros_ret_t _nros_check_ret = (call);                                                       \
        if (_nros_check_ret != NROS_RET_OK) {                                                      \
            NROS_CHECK_LOG(__FILE__, __LINE__, #call, _nros_check_ret);                            \
            return (retval);                                                                       \
        }                                                                                          \
    } while (0)

#endif /* NROS_CHECK_H */
