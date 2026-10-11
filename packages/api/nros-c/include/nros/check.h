/**
 * @file check.h
 * @ingroup grp_types
 * @brief Error-check convenience macros for the nros C API.
 *
 * Wraps the common "call returns rcl_ret_t; on failure log and bail"
 * pattern in one-liners so example main()s and user code don't open-code
 * 4-line check blocks at every API site.
 *
 * Pattern parity with `rclc`:
 *   - `RCLC_CHECK(call)`     — fatal: log and `return` from the
 *                              enclosing void function.
 *   - `RCLC_SOFTCHECK(call)` — non-fatal: log and continue.
 *
 * The macros log file:line, the literal call text, and the integer
 * return code. Issue 0332 — the default logger is `printf` ONLY on a hosted
 * target (`__STDC_HOSTED__`) or when `RCLC_CHECK_STDIO` is defined; on a
 * freestanding embedded target it defaults to a no-op, so a no_std C node does
 * not silently pull `<stdio.h>` into every TU.
 *
 * Override the log function by defining `RCLC_CHECK_LOG(file, line, expr,
 * ret)` before including this header (e.g. to route through a board's
 * UART, RTT, or `nros-log`), or define `RCLC_CHECK_STDIO` to force the `printf`
 * form on a freestanding target.
 *
 * Copyright 2026 nros contributors
 * Licensed under Apache-2.0
 */

#ifndef NROS_CHECK_H
#define NROS_CHECK_H

#include "nros/types.h"

#ifndef RCLC_CHECK_LOG
#if defined(RCLC_CHECK_STDIO) || (defined(__STDC_HOSTED__) && __STDC_HOSTED__)
#include <stdio.h>
#define RCLC_CHECK_LOG(file, line, expr, ret)                                                      \
    printf("[nros] %s:%d %s -> %d\n", (file), (line), (expr), (int)(ret))
#else
/* Issue 0332 — freestanding default: no-op, no `<stdio.h>` pulled. Define
   RCLC_CHECK_STDIO for the printf form, or RCLC_CHECK_LOG to route to a board
   UART / RTT / nros-log. The `(void)` casts keep -Wunused quiet at the sites. */
#define RCLC_CHECK_LOG(file, line, expr, ret)                                                      \
    ((void)(file), (void)(line), (void)(expr), (void)(ret))
#endif
#endif

/**
 * Fatal check: evaluate @p call once; if the returned `rcl_ret_t` is
 * not `RCL_RET_OK`, log the failure and `return` from the enclosing
 * function (which must return `void` or accept a bare `return;`).
 */
#define RCLC_CHECK(call)                                                                           \
    do {                                                                                           \
        rcl_ret_t _nros_check_ret = (call);                                                        \
        if (_nros_check_ret != RCL_RET_OK) {                                                       \
            RCLC_CHECK_LOG(__FILE__, __LINE__, #call, _nros_check_ret);                            \
            return;                                                                                \
        }                                                                                          \
    } while (0)

/**
 * Non-fatal check: evaluate @p call once; on failure, log and continue.
 * Use for "best effort" calls (e.g. publishing a heartbeat) where one
 * dropped iteration shouldn't tear down the node.
 */
#define RCLC_SOFTCHECK(call)                                                                       \
    do {                                                                                           \
        rcl_ret_t _nros_softcheck_ret = (call);                                                    \
        if (_nros_softcheck_ret != RCL_RET_OK) {                                                   \
            RCLC_CHECK_LOG(__FILE__, __LINE__, #call, _nros_softcheck_ret);                        \
        }                                                                                          \
    } while (0)

/**
 * Like `RCLC_CHECK` but for callers that need a custom return value
 * (non-`void` functions). Returns @p retval on failure.
 */
#define RCLC_CHECK_RET(call, retval)                                                               \
    do {                                                                                           \
        rcl_ret_t _nros_check_ret = (call);                                                        \
        if (_nros_check_ret != RCL_RET_OK) {                                                       \
            RCLC_CHECK_LOG(__FILE__, __LINE__, #call, _nros_check_ret);                            \
            return (retval);                                                                       \
        }                                                                                          \
    } while (0)

#endif /* NROS_CHECK_H */
