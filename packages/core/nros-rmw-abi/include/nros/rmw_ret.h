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
 *  - **Status only.** `rmw_ret_t` returned directly. `0` =
 *    success, negative = one of the named error codes.
 *  - **Byte count + error.** A non-negative return is the number of
 *    bytes produced; a negative return is one of the named error
 *    codes. Used by `try_recv_raw`, `try_recv_request`, `try_recv_reply_raw`.
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

/** Status code. Zero on success.
 *
 *  Phase 376 W3.d step B — the VALUES are upstream rmw's. `RMW_RET_OK` was
 *  already 0 on both sides; everything else moved from a negative code to
 *  upstream's positive one, so a status means the same number on both sides of
 *  the seam: OK 0, ERROR 1, TIMEOUT 2, UNSUPPORTED 3, BAD_ALLOC 10,
 *  INVALID_ARGUMENT 11, INCORRECT_RMW_IMPLEMENTATION 12,
 *  NODE_NAME_NON_EXISTENT 203.
 *
 *  (Written as prose, not an indented block: bindgen copies this comment into
 *  `generated.rs` verbatim, and rustdoc reads an indented block there as a Rust
 *  DOCTEST — which then fails to compile. Caught by `cargo test`.)
 *
 *  This is why step A had to come first. Eleven slots used to return a COUNT
 *  or a FLAG as a non-negative value and a status as a negative one; with
 *  `ERROR` at 1, a return of `1` would have meant both "one message" and
 *  "failed". Every one of those slots now reports through an out-parameter, so
 *  the sign carries nothing and the numbers are free to move.
 *
 *  Codes upstream does not define live in the EXTENSION RANGE at 1000+, so a
 *  future upstream addition can never collide with one of ours. That range is
 *  the one place we knowingly add to upstream's namespace.
 *
 *  Signedness is kept (`int32_t`, not an unsigned type) to match upstream's
 *  `rmw_ret_t` exactly. Nothing returns a negative value any more. */
typedef int32_t rmw_ret_t;

/** First value in the nano-ros extension range. Everything at or above this is
 *  ours; everything below it is upstream's or reserved for upstream. */
#define NROS_RMW_RET_EXTENSION_BASE        1000

/** Operation completed successfully. */
#define NROS_RMW_RET_OK                       0

/** Generic failure not covered by a more specific code. */
#define NROS_RMW_RET_ERROR                   1  /* upstream RMW_RET_ERROR */

/** Operation deadline elapsed before completion. */
#define NROS_RMW_RET_TIMEOUT                 2  /* upstream RMW_RET_TIMEOUT */

/**
 * Memory allocation failed.
 *
 * Returned by backends on `std` / `alloc`-equipped platforms when
 * heap allocation fails. Bare-metal backends generally do not return
 * this — they preallocate at session-open time.
 */
#define NROS_RMW_RET_BAD_ALLOC               10  /* upstream RMW_RET_BAD_ALLOC */

/** Caller supplied a NULL pointer, an out-of-range value, or an
 *  inconsistent argument combination. */
#define NROS_RMW_RET_INVALID_ARGUMENT        11  /* upstream RMW_RET_INVALID_ARGUMENT */

/** The backend does not implement this operation. Optional callbacks
 *  return this; the runtime then falls back to a default path. */
#define NROS_RMW_RET_UNSUPPORTED             3  /* upstream RMW_RET_UNSUPPORTED */

/** Publisher and subscription QoS profiles do not match in a way the
 *  backend cannot reconcile (e.g., reliable publisher vs. best-effort
 *  subscription on a backend that requires strict matching). */
#define NROS_RMW_RET_INCOMPATIBLE_QOS        (NROS_RMW_RET_EXTENSION_BASE + 0)

/** Topic, service, or action name failed validation (empty,
 *  non-printable bytes, illegal characters). */
#define NROS_RMW_RET_TOPIC_NAME_INVALID      (NROS_RMW_RET_EXTENSION_BASE + 1)

/** A request referenced a node that does not exist in this session. */
#define NROS_RMW_RET_NODE_NAME_NON_EXISTENT  203  /* upstream RMW_RET_NODE_NAME_NON_EXISTENT */

/** The backend does not support loaned messages on this entity, or
 *  the loan slot is currently in use. Caller may retry, or fall back
 *  to the copying path. */
#define NROS_RMW_RET_LOAN_NOT_SUPPORTED      (NROS_RMW_RET_EXTENSION_BASE + 2)

/** No data was available on a non-blocking receive. Distinct from
 *  `NROS_RMW_RET_TIMEOUT`: this fires immediately, not after a
 *  bounded wait. */
#define NROS_RMW_RET_NO_DATA                 (NROS_RMW_RET_EXTENSION_BASE + 3)

/** Resource (slot, queue, transport buffer) is momentarily
 *  unavailable. Caller should retry; never blocks. */
#define NROS_RMW_RET_WOULD_BLOCK             (NROS_RMW_RET_EXTENSION_BASE + 4)

/** Buffer supplied by the caller is smaller than the data the
 *  backend wants to deliver. */
#define NROS_RMW_RET_BUFFER_TOO_SMALL        (NROS_RMW_RET_EXTENSION_BASE + 5)

/** Incoming message exceeded the backend's static capacity. */
#define NROS_RMW_RET_MESSAGE_TOO_LARGE       (NROS_RMW_RET_EXTENSION_BASE + 6)

/** Phase 115.A.2 — caller-supplied versioned struct
 *  (e.g. `nros_transport_ops_t`) carries an `abi_version` the
 *  runtime does not understand. The previously installed copy (if
 *  any) is left untouched. */
#define NROS_RMW_RET_INCOMPATIBLE_ABI        (NROS_RMW_RET_EXTENSION_BASE + 7)

/** Phase 128.A.3 — `Executor::open` / `nros::init` could not pick a
 *  unique backend because no `nros-rmw-*` crate (or static lib) is
 *  linked into this binary. The walker found zero entries in the
 *  `.nros_rmw_init` section. */
#define NROS_RMW_RET_NO_BACKEND              (NROS_RMW_RET_EXTENSION_BASE + 8)

/** Phase 128.A.3 — more than one backend is linked into this
 *  binary and the caller did not select one. Set `NROS_RMW=<name>`
 *  (env var) to disambiguate, or use the bridge `Executor::open_multi`
 *  API to bind nodes to backends explicitly. */
#define NROS_RMW_RET_AMBIGUOUS_BACKEND       (NROS_RMW_RET_EXTENSION_BASE + 9)

/** Phase 128.A.3 — caller selected a backend by name (env var or
 *  `Executor::open_multi`) but no registered slot matches. The error
 *  is recoverable by linking the requested backend or correcting the
 *  spelling. */
#define NROS_RMW_RET_UNKNOWN_BACKEND         (NROS_RMW_RET_EXTENSION_BASE + 10)

/** Phase 155.B.3 — backend reached the wire but couldn't establish a
 *  session: refused TCP connect, unreachable agent, peer dropped the
 *  link mid-handshake. Distinct from `NROS_RMW_RET_ERROR` so callers
 *  (and the C-side `nros_support_init` / `nros_cpp_init` log lines)
 *  can distinguish "I can't reach the router" from "internal
 *  backend invariant tripped". Maps to / from
 *  `TransportError::ConnectionFailed` and `Disconnected`. */
#define NROS_RMW_RET_CONNECTION_FAILED       (NROS_RMW_RET_EXTENSION_BASE + 11)

/** Issue 0468 — a COMPILE-TIME capacity or configuration made the call
 *  impossible: the zenoh session pool (`ZPICO_MAX_SESSIONS`) or a
 *  publisher/queryable table is sized smaller than what the application
 *  asked for. Distinct from `NROS_RMW_RET_INVALID_ARGUMENT`, which means
 *  the CALLER passed something wrong — here the arguments are fine and the
 *  BUILD cannot honour them, so the remedy is a rebuild (or not asking for
 *  the extra resource), never a different argument.
 *
 *  Added because `TransportError::InvalidConfig` had no code of its own and
 *  encoded to `INVALID_ARGUMENT`, so an exhausted session pool arrived on
 *  the far side indistinguishable from a bad pointer — the last hop of the
 *  same collision that made issue 0465 read as a connection failure. Maps
 *  to / from `TransportError::InvalidConfig`. */
#define NROS_RMW_RET_INVALID_CONFIG          (NROS_RMW_RET_EXTENSION_BASE + 12)

/** Upstream `RMW_RET_INCORRECT_RMW_IMPLEMENTATION`. Defined for value parity
 *  even though a nano-ros image cannot raise it: upstream's rmw returns it when
 *  a handle's `implementation_identifier` does not match the loaded
 *  middleware, and an image links exactly one backend. Defined rather than
 *  omitted so the value can never be reused for something of ours — the whole
 *  point of pinning to upstream's numbering. */
#define NROS_RMW_RET_INCORRECT_RMW_IMPLEMENTATION 12  /* upstream, unreachable here */

#endif /* NROS_RMW_RET_H */
