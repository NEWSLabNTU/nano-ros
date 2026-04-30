#ifndef NROS_RMW_RET_H
#define NROS_RMW_RET_H

#include <stdint.h>

/**
 * @file rmw_ret.h
 * @brief Return-code constants for the nros RMW C vtable.
 *
 * Functions in `nros_rmw_vtable_t` and the public C entry points
 * (`nros_rmw_cffi_register`, …) report status as a signed 32-bit
 * integer. Zero means success; every error code is negative and
 * named by one of the macros below.
 *
 * Two return-shape conventions exist:
 *
 *  - **Status only.** `nros_rmw_ret_t` returned directly. `0` =
 *    success, negative = one of the named error codes.
 *  - **Byte count + error.** A non-negative return is the number of
 *    bytes produced; a negative return is one of the named error
 *    codes. Used by `try_recv_raw`, `try_recv_request`, `call_raw`.
 *
 * Pointer-returning calls (`open`, `create_publisher`, …) signal
 * failure with `NULL`; if the caller needs the specific failure
 * cause, it polls the session via the runtime API.
 *
 * No thread-local error string is exposed by the RMW layer — that
 * pattern requires thread-local heap storage which embedded targets
 * cannot afford. Backends log diagnostic strings at the failure
 * site via the platform's `printk`-equivalent.
 */

/** Signed 32-bit status code. Zero on success; negative on error. */
typedef int32_t nros_rmw_ret_t;

/** Operation completed successfully. */
#define NROS_RMW_RET_OK                       0

/** Generic failure not covered by a more specific code. */
#define NROS_RMW_RET_ERROR                   -1

/** Operation deadline elapsed before completion. */
#define NROS_RMW_RET_TIMEOUT                 -2

/**
 * Memory allocation failed.
 *
 * Returned by backends on `std` / `alloc`-equipped platforms when
 * heap allocation fails. Bare-metal backends generally do not return
 * this — they preallocate at session-open time.
 */
#define NROS_RMW_RET_BAD_ALLOC               -3

/** Caller supplied a NULL pointer, an out-of-range value, or an
 *  inconsistent argument combination. */
#define NROS_RMW_RET_INVALID_ARGUMENT        -4

/** The backend does not implement this operation. Optional callbacks
 *  return this; the runtime then falls back to a default path. */
#define NROS_RMW_RET_UNSUPPORTED             -5

/** Publisher and subscriber QoS profiles do not match in a way the
 *  backend cannot reconcile (e.g., reliable publisher vs. best-effort
 *  subscriber on a backend that requires strict matching). */
#define NROS_RMW_RET_INCOMPATIBLE_QOS        -6

/** Topic, service, or action name failed validation (empty,
 *  non-printable bytes, illegal characters). */
#define NROS_RMW_RET_TOPIC_NAME_INVALID      -7

/** A request referenced a node that does not exist in this session. */
#define NROS_RMW_RET_NODE_NAME_NON_EXISTENT  -8

/** The backend does not support loaned messages on this entity, or
 *  the loan slot is currently in use. Caller may retry, or fall back
 *  to the copying path. */
#define NROS_RMW_RET_LOAN_NOT_SUPPORTED      -9

/** No data was available on a non-blocking receive. Distinct from
 *  `NROS_RMW_RET_TIMEOUT`: this fires immediately, not after a
 *  bounded wait. */
#define NROS_RMW_RET_NO_DATA                -10

/** Resource (slot, queue, transport buffer) is momentarily
 *  unavailable. Caller should retry; never blocks. */
#define NROS_RMW_RET_WOULD_BLOCK            -11

/** Buffer supplied by the caller is smaller than the data the
 *  backend wants to deliver. */
#define NROS_RMW_RET_BUFFER_TOO_SMALL       -12

/** Incoming message exceeded the backend's static capacity. */
#define NROS_RMW_RET_MESSAGE_TOO_LARGE      -13

#endif /* NROS_RMW_RET_H */
